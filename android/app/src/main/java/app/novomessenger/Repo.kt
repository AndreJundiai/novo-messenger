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
    private val prefs = context.getSharedPreferences("nm", Context.MODE_PRIVATE)

    var messenger: Messenger? = null
        private set

    /** Aumenta a cada mudança, para a interface se redesenhar. */
    val revision = MutableStateFlow(0)

    /** Aparelhos conectados por perto (Bluetooth/Wi-Fi direto). */
    val peers = MutableStateFlow(0)

    /** Relays de internet conectados. */
    val relays = MutableStateFlow(0)

    /** Usar a internet (relays Nostr) quando houver conexão. */
    val internetEnabled = MutableStateFlow(prefs.getBoolean("internet", true))

    /** Diretório: resumo do número → cartão de quem tem o app (vindo dos relays). */
    val directory = MutableStateFlow<Map<String, String>>(emptyMap())

    /** Resumos já calculados (o cálculo é lento de propósito). */
    private val hashCache = java.util.concurrent.ConcurrentHashMap<String, String>()

    /** O app está na tela (para não notificar à toa). */
    @Volatile var foreground = false

    /** Conversa aberta agora, se houver. */
    @Volatile var openChat: String? = null

    /** O serviço de rede chama isto para mandar o que estiver na caixa de saída. */
    @Volatile var onOutboxChanged: (() -> Unit)? = null

    /** O serviço de rede liga ou desliga a internet quando a preferência muda. */
    @Volatile var onInternetToggled: ((Boolean) -> Unit)? = null

    /** O serviço de rede manda este pedido aos relays (busca no diretório). */
    @Volatile var onRelaySend: ((String) -> Boolean)? = null

    /** O serviço de rede republica minha entrada no diretório quando ela muda. */
    @Volatile var onDirectoryChanged: (() -> Unit)? = null

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

    private fun nowMs(): ULong = System.currentTimeMillis().toULong()

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

    private inline fun <T> attempt(block: (Messenger) -> T): Result<T> {
        val m = messenger ?: return Result.failure(IllegalStateException("sem identidade"))
        return try {
            Result.success(block(m))
        } catch (e: NmException) {
            Result.failure(e)
        }
    }

    /** Executa uma ação que muda a conversa e manda o resultado pela rede. */
    private inline fun <T> action(block: (Messenger) -> T): Result<T> =
        attempt(block).onSuccess {
            changed()
            onOutboxChanged?.invoke()
        }

    // ---------- identidade ----------

    fun myName(): String = messenger?.myName().orEmpty()
    fun myCard(): String = messenger?.myCard().orEmpty()
    fun mySafetyCode(): String = messenger?.mySafetyCode().orEmpty()
    fun myId(): String = messenger?.myId().orEmpty()

    fun setMyName(name: String) {
        messenger?.setMyName(name)
        changed()
    }

    // ---------- contatos e escudo ----------

    fun contacts(): List<ContactInfo> = messenger?.contacts().orEmpty()
    fun contact(peerId: String): ContactInfo? = messenger?.contact(peerId)

    /** Contatos com nome parecido ao do código lido (golpe do "troquei de número"). */
    fun similarContacts(card: String): List<ContactInfo> = messenger?.similarContacts(card).orEmpty()

    fun addContact(card: String): Result<ContactInfo> = attempt { it.addContact(card) }.onSuccess { changed() }

    fun moneyAlerts(): Boolean = messenger?.moneyAlerts() ?: true

    fun setMoneyAlerts(on: Boolean) {
        messenger?.setMoneyAlerts(on)
        changed()
    }

    fun dismissAlert(id: ULong) {
        if (messenger?.dismissAlert(id) == true) changed()
    }

    // ---------- conversas ----------

    fun conversations(): List<ConversationInfo> = messenger?.conversations().orEmpty()

    fun messages(peerId: String): List<MessageInfo> = attempt { it.messages(peerId) }.getOrDefault(emptyList())

    fun search(query: String): List<MessageInfo> = messenger?.search(query).orEmpty()

    fun send(peerId: String, text: String, replyTo: ULong?): Result<MessageInfo> = action {
        if (replyTo != null) it.sendReply(peerId, text, replyTo, nowMs()) else it.sendText(peerId, text, nowMs())
    }

    fun edit(id: ULong, text: String): Result<Unit> = action { it.editMessage(id, text, nowMs()) }

    fun deleteForEveryone(id: ULong): Result<Unit> = action { it.deleteForEveryone(id, nowMs()) }

    fun deleteForMe(id: ULong) {
        if (messenger?.deleteForMe(id) == true) changed()
    }

    fun react(id: ULong, emoji: String): Result<Unit> = action { it.react(id, emoji, nowMs()) }

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

    // ---------- backup ----------

    fun exportBackup(password: String): Result<ByteArray> = attempt { it.exportBackup(password) }

    /** Troca tudo deste aparelho pelo conteúdo do backup. */
    fun restoreBackup(data: ByteArray, password: String): Result<Unit> = try {
        messenger = Messenger.restoreBackup(data, password)
        changed()
        onOutboxChanged?.invoke()
        Result.success(Unit)
    } catch (e: NmException) {
        Result.failure(e)
    }

    // ---------- agenda e diretório ----------

    /** Meu número (opcional, só dígitos com código do país). */
    fun myPhone(): String = prefs.getString("my_phone", "").orEmpty()

    /** Aparecer para quem tem meu número salvo. Desligado por padrão. */
    fun discoverable(): Boolean = prefs.getBoolean("discoverable", false)

    /** Salva meu número; devolve falso se não parecer um número. */
    fun setMyPhone(raw: String): Boolean {
        val n = if (raw.isBlank()) "" else uniffi.nm_ffi.normalizePhone(raw) ?: return false
        prefs.edit().putString("my_phone", n).apply()
        onDirectoryChanged?.invoke()
        return true
    }

    fun setDiscoverable(on: Boolean) {
        prefs.edit().putBoolean("discoverable", on).apply()
        onDirectoryChanged?.invoke()
        revision.value += 1
    }

    fun phoneHash(e164: String): String = hashCache.getOrPut(e164) { uniffi.nm_ffi.phoneHash(e164) }

    /**
     * Evento do diretório a publicar agora, ou nada. Também apaga a entrada
     * antiga quando a pessoa desliga a opção ou troca de número.
     */
    fun pendingDirectoryEvents(): List<String> {
        val m = messenger ?: return emptyList()
        val now = (System.currentTimeMillis() / 1000).toULong()
        val out = mutableListOf<String>()
        val published = prefs.getString("dir_published", "").orEmpty()
        val wanted = if (discoverable() && myPhone().isNotEmpty()) phoneHash(myPhone()) else ""
        if (published.isNotEmpty() && published != wanted) out += m.directoryEvent(published, false, now)
        if (wanted.isNotEmpty()) out += m.directoryEvent(wanted, true, now)
        prefs.edit().putString("dir_published", wanted).apply()
        return out
    }

    /** Procura no diretório quem destes números tem o app. Devolve falso sem internet. */
    fun queryDirectory(hashes: List<String>): Boolean {
        val send = onRelaySend ?: return false
        var any = false
        hashes.chunked(100).forEachIndexed { i, chunk ->
            if (send(uniffi.nm_ffi.directoryRequest("dir$i", chunk))) any = true
        }
        return any
    }

    fun onDirectory(hash: String, card: String) {
        val cur = directory.value
        directory.value = if (card.isEmpty()) cur - hash else cur + (hash to card)
    }

    fun addFoundContact(card: String, phone: String): Result<ContactInfo> =
        attempt { it.addFoundContact(card, phone) }.onSuccess { changed() }

    /** Texto do SMS de confirmação para o número do contato. */
    fun verificationSms(peerId: String): Result<String> = attempt { code ->
        val text = code.verificationCode(peerId)
        changed()
        "Novo Messenger: ${myName()} quer confirmar que este número é seu. Abra o app, toque em + → Colar código e cole: $text\n(Não é cobrança nem pedido de dinheiro.)"
    }

    /** Código colado pela pessoa (cartão ou confirmação de SMS). */
    fun acceptCode(text: String): Result<ContactInfo> = action { it.acceptCode(text, nowMs()) }

    fun inviteSms(): String =
        "Oi! Estou usando o Novo Messenger, que funciona com e sem internet. Baixe em $INVITE_URL " +
            "e, no app, toque em + → Colar código e cole o meu: ${myCard()}"

    // ---------- internet ----------

    fun setInternetEnabled(on: Boolean) {
        prefs.edit().putBoolean("internet", on).apply()
        internetEnabled.value = on
        onInternetToggled?.invoke(on)
    }

    /** Pedido aos relays: mensagens desde a última vista (com 1 h de folga), ou dos últimos 7 dias. */
    fun relayRequest(subscription: String): String {
        val nowS = System.currentTimeMillis() / 1000
        val last = prefs.getLong("relay_seen", 0L)
        val since = if (last > 0) last - 3600 else nowS - 7 * 24 * 3600
        return messenger?.relayRequest(subscription, since.coerceAtLeast(0).toULong()).orEmpty()
    }

    fun noteRelayTime(createdAt: ULong) {
        val t = createdAt.toLong()
        if (t > prefs.getLong("relay_seen", 0L)) prefs.edit().putLong("relay_seen", t).apply()
    }

    fun relayEvent(toPeerId: String, data: ByteArray): String? =
        attempt { it.relayEvent(toPeerId, data, (System.currentTimeMillis() / 1000).toULong()) }.getOrNull()

    // ---------- transporte ----------

    fun hasOutgoing(): Boolean = messenger?.hasOutgoing() == true

    fun outbox(): List<OutgoingPacket> = messenger?.outbox().orEmpty()

    fun markSent(id: ULong) {
        if (messenger?.markSent(id) == true) changed()
    }

    /** Envelope que chegou por algum transporte. */
    fun receive(data: ByteArray) {
        val m = messenger ?: return
        val r = m.receive(data, nowMs())
        val msg = r.message
        r.contactVerified?.let { id ->
            changed()
            val name = contact(id)?.name ?: "Contato"
            Notifications.message(context, id, name, "$name confirmou o número pelo SMS ✓")
        }
        if (msg == null && r.updated.isEmpty() && r.changed.isEmpty()) {
            // Cópia repetida: pode haver confirmação nova para mandar.
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
                val text = if (msg.moneyAlert) "⚠ Pedido de dinheiro: ${msg.text}" else msg.text
                Notifications.message(context, msg.peerId, name, text)
            }
        }
        onOutboxChanged?.invoke()
    }

    companion object {
        private const val TAG = "NovoMessenger"
        const val INVITE_URL = "https://github.com/AndreJundiai/novo-messenger"
    }
}
