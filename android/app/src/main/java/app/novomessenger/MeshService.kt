package app.novomessenger

import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import android.util.Log
import androidx.core.app.ServiceCompat
import com.google.android.gms.nearby.Nearby
import com.google.android.gms.nearby.connection.AdvertisingOptions
import com.google.android.gms.nearby.connection.ConnectionInfo
import com.google.android.gms.nearby.connection.ConnectionLifecycleCallback
import com.google.android.gms.nearby.connection.ConnectionResolution
import com.google.android.gms.nearby.connection.ConnectionsClient
import com.google.android.gms.nearby.connection.DiscoveredEndpointInfo
import com.google.android.gms.nearby.connection.DiscoveryOptions
import com.google.android.gms.nearby.connection.EndpointDiscoveryCallback
import com.google.android.gms.nearby.connection.Payload
import com.google.android.gms.nearby.connection.PayloadCallback
import com.google.android.gms.nearby.connection.PayloadTransferUpdate
import com.google.android.gms.nearby.connection.Strategy

/**
 * Mantém as duas redes ligadas e manda a caixa de saída por todas que
 * estiverem disponíveis:
 *
 * - **Por perto:** Bluetooth e Wi-Fi direto (Nearby Connections), sem internet.
 * - **Internet:** relays Nostr públicos ([NostrClient]), quando houver conexão.
 *
 * Os envelopes já vêm cifrados do núcleo Rust; o mesmo envelope pode chegar
 * pelos dois caminhos, e o núcleo descarta a cópia.
 */
class MeshService : Service() {
    private lateinit var client: ConnectionsClient
    private lateinit var nostr: NostrClient
    private val main = Handler(Looper.getMainLooper())
    private val connected = mutableSetOf<String>()
    private val connecting = mutableSetOf<String>()
    private val repo get() = (application as NmApplication).repo
    private var running = false
    private var advertising = false
    private var discovering = false

    /** Quando cada envelope pendente foi publicado na internet pela última vez (evita repetir). */
    private val publishedAt = HashMap<Int, Long>()

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        Notifications.ensureChannels(this)
        client = Nearby.getConnectionsClient(this)
        nostr = NostrClient(repo, onEnvelope = { repo.receive(it) }, onConnected = { flush(); publishDirectory() })
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val type = if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0
        try {
            ServiceCompat.startForeground(this, Notifications.MESH_ID, Notifications.mesh(this, connected.size, nostr.connectedCount), type)
        } catch (e: Exception) {
            Log.e(TAG, "não foi possível iniciar em primeiro plano", e)
            stopSelf()
            return START_NOT_STICKY
        }
        if (!running) {
            running = true
            repo.onOutboxChanged = { main.post { flush() } }
            repo.onInternetToggled = { on -> main.post { if (on) nostr.start() else nostr.stop() } }
            repo.onRelaySend = { text -> nostr.publish(text) }
            repo.onDirectoryChanged = { main.post { publishDirectory() } }
            if (repo.internetEnabled.value) nostr.start()
            main.postDelayed(retry, RETRY_MS)
        }
        // Chamado de novo quando a pessoa dá a permissão de Bluetooth depois.
        startNearby()
        return START_STICKY
    }

    /** De tempos em tempos, tenta de novo o que ainda não foi confirmado. */
    private val retry = object : Runnable {
        override fun run() {
            if (repo.hasOutgoing()) flush()
            updateNotification()
            main.postDelayed(this, RETRY_MS)
        }
    }

    private fun myEndpointName(): String = repo.myId().take(8).ifEmpty { "nm" }

    /** Liga a conexão por perto. Sem permissão de Bluetooth, falha em silêncio e a internet segue. */
    private fun startNearby() {
        val strategy = Strategy.P2P_CLUSTER
        if (!advertising) {
            client.startAdvertising(myEndpointName(), SERVICE_ID, lifecycle, AdvertisingOptions.Builder().setStrategy(strategy).build())
                .addOnSuccessListener { advertising = true }
                .addOnFailureListener { Log.w(TAG, "anúncio falhou", it) }
        }
        if (!discovering) {
            client.startDiscovery(SERVICE_ID, discovery, DiscoveryOptions.Builder().setStrategy(strategy).build())
                .addOnSuccessListener { discovering = true }
                .addOnFailureListener { Log.w(TAG, "busca falhou", it) }
        }
    }

    private val discovery = object : EndpointDiscoveryCallback() {
        override fun onEndpointFound(endpointId: String, info: DiscoveredEndpointInfo) {
            if (info.serviceId != SERVICE_ID || endpointId in connected || endpointId in connecting) return
            // Só um dos lados pede a conexão, para não colidir: o de nome "menor".
            if (myEndpointName() > info.endpointName) return
            connecting += endpointId
            client.requestConnection(myEndpointName(), endpointId, lifecycle)
                .addOnFailureListener { connecting -= endpointId }
        }

        override fun onEndpointLost(endpointId: String) {
            connecting -= endpointId
        }
    }

    private val lifecycle = object : ConnectionLifecycleCallback() {
        override fun onConnectionInitiated(endpointId: String, info: ConnectionInfo) {
            client.acceptConnection(endpointId, payloads)
        }

        override fun onConnectionResult(endpointId: String, result: ConnectionResolution) {
            connecting -= endpointId
            if (result.status.isSuccess) {
                connected += endpointId
                updatePeers()
                flush(onlyNearby = listOf(endpointId))
            }
        }

        override fun onDisconnected(endpointId: String) {
            connected -= endpointId
            updatePeers()
        }
    }

    private val payloads = object : PayloadCallback() {
        override fun onPayloadReceived(endpointId: String, payload: Payload) {
            if (payload.type != Payload.Type.BYTES) return
            val bytes = payload.asBytes() ?: return
            if (bytes.size > 1 && bytes[0] == KIND_ENVELOPE) {
                repo.receive(bytes.copyOfRange(1, bytes.size))
            }
        }

        override fun onPayloadTransferUpdate(endpointId: String, update: PayloadTransferUpdate) {}
    }

    /**
     * Manda a caixa de saída. Com [onlyNearby], manda só para esses aparelhos
     * (uma conexão nova); senão, para todos os aparelhos e para a internet.
     */
    private fun flush(onlyNearby: List<String>? = null) {
        val nearby = onlyNearby ?: connected.toList()
        val internet = onlyNearby == null && nostr.connectedCount > 0
        // Sem nenhum caminho, não esvazia a caixa (as confirmações saem uma vez só).
        if (nearby.isEmpty() && !internet) return
        val now = System.currentTimeMillis()
        for (packet in repo.outbox()) {
            var sent = false
            if (nearby.isNotEmpty()) {
                val bytes = ByteArray(packet.data.size + 1)
                bytes[0] = KIND_ENVELOPE
                System.arraycopy(packet.data, 0, bytes, 1, packet.data.size)
                client.sendPayload(nearby, Payload.fromBytes(bytes))
                    .addOnSuccessListener { packet.messageId?.let { repo.markSent(it) } }
                    .addOnFailureListener { Log.w(TAG, "envio por perto falhou", it) }
            }
            if (internet) {
                // Pendentes são repetidos na internet no máximo a cada 10 minutos.
                val key = packet.data.contentHashCode()
                val last = publishedAt[key] ?: 0L
                if (now - last >= REPUBLISH_MS) {
                    val ev = repo.relayEvent(packet.toPeerId, packet.data)
                    if (ev != null && nostr.publish(ev)) {
                        publishedAt[key] = now
                        sent = true
                    }
                }
            }
            if (sent) packet.messageId?.let { repo.markSent(it) }
        }
        if (publishedAt.size > 2000) publishedAt.entries.removeIf { now - it.value > REPUBLISH_MS }
    }

    /** Publica (ou apaga) minha entrada no diretório. O resumo é lento: calcula fora da tela. */
    private fun publishDirectory() {
        if (nostr.connectedCount == 0) return
        Thread {
            val events = repo.pendingDirectoryEvents()
            main.post { events.forEach { nostr.publish(it) } }
        }.start()
    }

    private fun updatePeers() {
        repo.peers.value = connected.size
        updateNotification()
    }

    private fun updateNotification() {
        val nm = getSystemService(android.app.NotificationManager::class.java)
        nm.notify(Notifications.MESH_ID, Notifications.mesh(this, connected.size, nostr.connectedCount))
    }

    override fun onDestroy() {
        main.removeCallbacksAndMessages(null)
        repo.onOutboxChanged = null
        repo.onInternetToggled = null
        repo.onRelaySend = null
        repo.onDirectoryChanged = null
        repo.peers.value = 0
        nostr.stop()
        client.stopAdvertising()
        client.stopDiscovery()
        client.stopAllEndpoints()
        super.onDestroy()
    }

    companion object {
        private const val TAG = "NmMesh"
        private const val SERVICE_ID = "app.novomessenger.mesh"
        private const val KIND_ENVELOPE: Byte = 1
        private const val RETRY_MS = 120_000L
        private const val REPUBLISH_MS = 600_000L
    }
}
