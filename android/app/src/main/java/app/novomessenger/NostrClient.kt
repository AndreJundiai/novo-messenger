package app.novomessenger

import android.os.Handler
import android.os.Looper
import android.util.Log
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import uniffi.nm_ffi.RelayKind
import uniffi.nm_ffi.defaultRelays
import uniffi.nm_ffi.parseRelayMessage
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.TimeUnit

/**
 * Conexão com relays Nostr públicos pela internet.
 *
 * Os envelopes já saem cifrados do núcleo Rust: o relay guarda e repassa
 * bytes que não consegue ler. Se a internet cair, reconecta sozinho, com
 * espera crescente (5 s, 10 s, 20 s… até 5 min).
 */
class NostrClient(
    private val repo: Repo,
    private val onEnvelope: (ByteArray) -> Unit,
    private val onConnected: () -> Unit,
) {
    private val http = OkHttpClient.Builder()
        .pingInterval(30, TimeUnit.SECONDS)
        .connectTimeout(15, TimeUnit.SECONDS)
        .build()
    private val main = Handler(Looper.getMainLooper())
    private val open = ConcurrentHashMap<String, WebSocket>()
    private val attempts = ConcurrentHashMap<String, Int>()
    @Volatile private var running = false

    val connectedCount: Int get() = open.size

    fun start() {
        if (running) return
        running = true
        defaultRelays().forEach { connect(it) }
    }

    fun stop() {
        running = false
        main.removeCallbacksAndMessages(null)
        open.values.forEach { it.close(1000, null) }
        open.clear()
        repo.relays.value = 0
    }

    private fun connect(url: String) {
        if (!running) return
        http.newWebSocket(Request.Builder().url(url).build(), Listener(url))
    }

    private inner class Listener(private val url: String) : WebSocketListener() {
        override fun onOpen(webSocket: WebSocket, response: Response) {
            main.post {
                if (!running) {
                    webSocket.close(1000, null)
                    return@post
                }
                open[url] = webSocket
                attempts[url] = 0
                repo.relays.value = open.size
                webSocket.send(repo.relayRequest(SUBSCRIPTION))
                onConnected()
            }
        }

        override fun onMessage(webSocket: WebSocket, text: String) {
            val e = parseRelayMessage(text)
            when (e.kind) {
                RelayKind.ENVELOPE -> {
                    val data = e.data ?: return
                    main.post {
                        repo.noteRelayTime(e.createdAt)
                        onEnvelope(data)
                    }
                }
                RelayKind.ACCEPTED -> if (!e.ok) Log.w(TAG, "$url recusou: ${e.info}")
                RelayKind.NOTICE -> Log.i(TAG, "$url avisa: ${e.info}")
                else -> {}
            }
        }

        override fun onClosed(webSocket: WebSocket, code: Int, reason: String) = lost()

        override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
            Log.i(TAG, "$url sem conexão: ${t.message}")
            lost()
        }

        private fun lost() {
            main.post {
                open.remove(url)
                repo.relays.value = open.size
                if (!running) return@post
                val n = (attempts[url] ?: 0) + 1
                attempts[url] = n
                val delay = minOf(5_000L shl minOf(n - 1, 6), 300_000L)
                main.postDelayed({ connect(url) }, delay)
            }
        }
    }

    /** Manda um evento para todos os relays conectados. Devolve se algum aceitou na fila. */
    fun publish(eventJson: String): Boolean {
        var any = false
        for (ws in open.values) {
            if (ws.send(eventJson)) any = true
        }
        return any
    }

    companion object {
        private const val TAG = "NmNostr"
        private const val SUBSCRIPTION = "nm"
    }
}
