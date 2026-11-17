package app.novomessenger

import android.app.Service
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
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
 * Mantém o celular conectado a outros aparelhos com o app por perto, por
 * Bluetooth e Wi-Fi direto (Nearby Connections), sem internet.
 *
 * Ao conectar, cada lado manda o que tem na caixa de saída. Os envelopes já
 * vêm cifrados do núcleo Rust: a conexão em si não precisa ser confiável.
 */
class MeshService : Service() {
    private lateinit var client: ConnectionsClient
    private val connected = mutableSetOf<String>()
    private val connecting = mutableSetOf<String>()
    private val repo get() = (application as NmApplication).repo
    private var running = false

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        Notifications.ensureChannels(this)
        client = Nearby.getConnectionsClient(this)
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val type = if (Build.VERSION.SDK_INT >= 29) ServiceInfo.FOREGROUND_SERVICE_TYPE_CONNECTED_DEVICE else 0
        try {
            ServiceCompat.startForeground(this, Notifications.MESH_ID, Notifications.mesh(this, connected.size), type)
        } catch (e: Exception) {
            Log.e(TAG, "não foi possível iniciar em primeiro plano", e)
            stopSelf()
            return START_NOT_STICKY
        }
        if (!running) {
            running = true
            repo.onOutboxChanged = { flush(connected.toList()) }
            start()
        }
        return START_STICKY
    }

    private fun myEndpointName(): String = repo.myId().take(8).ifEmpty { "nm" }

    private fun start() {
        val strategy = Strategy.P2P_CLUSTER
        client.startAdvertising(myEndpointName(), SERVICE_ID, lifecycle, AdvertisingOptions.Builder().setStrategy(strategy).build())
            .addOnFailureListener { Log.w(TAG, "anúncio falhou", it) }
        client.startDiscovery(SERVICE_ID, discovery, DiscoveryOptions.Builder().setStrategy(strategy).build())
            .addOnFailureListener { Log.w(TAG, "busca falhou", it) }
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
                flush(listOf(endpointId))
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

    /** Manda a caixa de saída para os aparelhos indicados. */
    private fun flush(endpoints: List<String>) {
        // Sem ninguém conectado, não esvazia a caixa (as confirmações saem uma vez só).
        if (endpoints.isEmpty()) return
        for (packet in repo.outbox()) {
            val bytes = ByteArray(packet.data.size + 1)
            bytes[0] = KIND_ENVELOPE
            System.arraycopy(packet.data, 0, bytes, 1, packet.data.size)
            client.sendPayload(endpoints, Payload.fromBytes(bytes))
                .addOnSuccessListener { packet.messageId?.let { repo.markSent(it) } }
                .addOnFailureListener { Log.w(TAG, "envio falhou", it) }
        }
    }

    private fun updatePeers() {
        repo.peers.value = connected.size
        val nm = getSystemService(android.app.NotificationManager::class.java)
        nm.notify(Notifications.MESH_ID, Notifications.mesh(this, connected.size))
    }

    override fun onDestroy() {
        repo.onOutboxChanged = null
        repo.peers.value = 0
        client.stopAdvertising()
        client.stopDiscovery()
        client.stopAllEndpoints()
        super.onDestroy()
    }

    companion object {
        private const val TAG = "NmMesh"
        private const val SERVICE_ID = "app.novomessenger.mesh"
        private const val KIND_ENVELOPE: Byte = 1
    }
}
