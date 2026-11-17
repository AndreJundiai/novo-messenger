package app.novomessenger

import android.content.Context
import android.util.Log
import kotlinx.coroutines.flow.MutableStateFlow
import uniffi.nm_ffi.ContactInfo
import uniffi.nm_ffi.ConversationInfo
import uniffi.nm_ffi.MessageInfo
import uniffi.nm_ffi.Messenger
import uniffi.nm_ffi.NmException
import uniffi.nm_ffi.OutgoingPacket
import java.io.File
import java.util.concurrent.Executors

/**
 * Acesso único ao núcleo Rust ([Messenger]) e ao arquivo salvo.
 * A interface observa [revision]: muda sempre que algo na conversa muda.
 */
class Repo(private val context: Context) {
    private val file = File(context.filesDir, "nm-state.bin")
    private val io = Executors.newSingleThreadExecutor()

    var messenger: Messenger? = null
        private set

    /** Aumenta a cada mudança, para a interface se redesenhar. */
    val revision = MutableStateFlow(0)

    /** Quantos aparelhos estão conectados agora. */
    val peers = MutableStateFlow(0)

    /** O app está na tela (para não notificar à toa). */
    @Volatile var foreground = false

    /** Conversa aberta agora, se houver. */
    @Volatile var openChat: String? = null

    /** O serviço de rede chama isto para mandar o que estiver na caixa de saída. */
    @Volatile var onOutboxChanged: (() -> Unit)? = null

    init {
        if (file.exists()) {
            messenger = try {
                Messenger.restore(file.readBytes())
            } catch (e: Exception) {
                Log.e(TAG, "estado salvo ilegível; guardando cópia e começando do zero", e)
                file.renameTo(File(context.filesDir, "nm-state.bad"))
                null
            }
        }
    }

    fun hasIdentity(): Boolean = messenger != null

    fun createIdentity(name: String) {
        messenger = Messenger.create(name)
        changed()
    }

    private fun now(): ULong = System.currentTimeMillis().toULong()

    /** Salva em segundo plano, trocando o arquivo de uma vez (sem arquivo pela metade). */
    private fun changed() {
        revision.value += 1
        val data = messenger?.save() ?: return
        io.execute {
            val tmp = File(context.filesDir, "nm-state.tmp")
            tmp.writeBytes(data)
            if (!tmp.renameTo(file)) {
                file.delete()
                tmp.renameTo(file)
            }
        }
    }

    fun myName(): String = messenger?.myName().orEmpty()
    fun myCard(): String = messenger?.myCard().orEmpty()
    fun mySafetyCode(): String = messenger?.mySafetyCode().orEmpty()
    fun myId(): String = messenger?.myId().orEmpty()

    fun conversations(): List<ConversationInfo> = messenger?.conversations().orEmpty()
    fun contacts(): List<ContactInfo> = messenger?.contacts().orEmpty()
    fun contact(peerId: String): ContactInfo? = messenger?.contact(peerId)

    fun messages(peerId: String): List<MessageInfo> = try {
        messenger?.messages(peerId).orEmpty()
    } catch (e: NmException) {
        emptyList()
    }

    fun addContact(card: String): Result<ContactInfo> {
        val m = messenger ?: return Result.failure(IllegalStateException("sem identidade"))
        return try {
            val c = m.addContact(card)
            changed()
            Result.success(c)
        } catch (e: NmException) {
            Result.failure(e)
        }
    }

    fun send(peerId: String, text: String): Result<MessageInfo> {
        val m = messenger ?: return Result.failure(IllegalStateException("sem identidade"))
        return try {
            val msg = m.sendText(peerId, text, now())
            changed()
            onOutboxChanged?.invoke()
            Result.success(msg)
        } catch (e: NmException) {
            Result.failure(e)
        }
    }

    fun markRead(peerId: String) {
        val m = messenger ?: return
        try {
            if (m.markRead(peerId) > 0u) {
                changed()
                onOutboxChanged?.invoke()
            }
        } catch (_: NmException) {
        }
    }

    fun outbox(): List<OutgoingPacket> = messenger?.outbox().orEmpty()

    fun markSent(id: ULong) {
        if (messenger?.markSent(id) == true) changed()
    }

    /** Envelope que chegou por algum transporte. */
    fun receive(data: ByteArray) {
        val m = messenger ?: return
        val r = m.receive(data, now())
        val msg = r.message
        if (msg == null && r.updated.isEmpty()) {
            // Cópia repetida ou confirmação já conhecida: pode haver confirmação nova para mandar.
            onOutboxChanged?.invoke()
            return
        }
        changed()
        if (msg != null) {
            val reading = foreground && openChat == msg.peerId
            if (reading) {
                markRead(msg.peerId)
            } else {
                val name = contact(msg.peerId)?.name ?: "Contato"
                Notifications.message(context, msg.peerId, name, msg.text)
            }
        }
        onOutboxChanged?.invoke()
    }

    companion object {
        private const val TAG = "NovoMessenger"
    }
}
