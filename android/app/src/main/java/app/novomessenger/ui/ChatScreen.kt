package app.novomessenger.ui

import android.widget.Toast
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Close
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TextFieldDefaults
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import app.novomessenger.Repo
import uniffi.nm_ffi.MessageInfo

private val QUICK_REACTIONS = listOf("👍", "❤️", "😂", "😮", "😢", "🙏")

private val AlertBg = Color(0xFFFFF1C2)
private val AlertFg = Color(0xFF5A4300)

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(repo: Repo, revision: Int, peers: Int, relays: Int, peerId: String, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val clipboard = LocalClipboardManager.current
    val contact = remember(revision, peerId) { repo.contact(peerId) }
    val messages = remember(revision, peerId) { repo.messages(peerId) }
    val byId = remember(messages) { messages.associateBy { it.id } }
    var text by rememberSaveable { mutableStateOf("") }
    var replyTo by remember { mutableStateOf<MessageInfo?>(null) }
    var editing by remember { mutableStateOf<MessageInfo?>(null) }
    var menuFor by remember { mutableStateOf<MessageInfo?>(null) }
    val list = rememberLazyListState()
    val colors = LocalChatColors.current

    DisposableEffect(peerId) {
        repo.openChat = peerId
        onDispose { repo.openChat = null }
    }
    LaunchedEffect(revision, peerId) { repo.markRead(peerId) }
    LaunchedEffect(messages.size) { if (messages.isNotEmpty()) list.animateScrollToItem(messages.size - 1) }

    fun toast(t: Throwable) = Toast.makeText(ctx, t.message ?: "não foi possível", Toast.LENGTH_SHORT).show()

    fun submit() {
        val e = editing
        if (e != null) {
            repo.edit(e.id, text).fold(onSuccess = { text = ""; editing = null }, onFailure = ::toast)
        } else {
            repo.send(peerId, text, replyTo?.id).fold(onSuccess = { text = ""; replyTo = null }, onFailure = ::toast)
        }
    }

    menuFor?.let { m ->
        ModalBottomSheet(onDismissRequest = { menuFor = null }) {
            Column(Modifier.fillMaxWidth().navigationBarsPadding().padding(bottom = 12.dp)) {
                if (!m.deleted) {
                    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp), horizontalArrangement = Arrangement.SpaceEvenly) {
                        QUICK_REACTIONS.forEach { emoji ->
                            val chosen = m.myReaction == emoji
                            Box(
                                Modifier
                                    .size(48.dp)
                                    .background(if (chosen) MaterialTheme.colorScheme.primaryContainer else Color.Transparent, CircleShape)
                                    .clickable {
                                        repo.react(m.id, if (chosen) "" else emoji).onFailure(::toast)
                                        menuFor = null
                                    }
                                    .semantics { contentDescription = if (chosen) "Tirar reação $emoji" else "Reagir com $emoji" },
                                contentAlignment = Alignment.Center,
                            ) { Text(emoji, fontSize = 26.sp) }
                        }
                    }
                    HorizontalDivider()
                    SheetAction("Responder") { replyTo = m; editing = null; menuFor = null }
                    SheetAction("Copiar") {
                        clipboard.setText(AnnotatedString(m.text))
                        Toast.makeText(ctx, "Copiado", Toast.LENGTH_SHORT).show()
                        menuFor = null
                    }
                    if (m.outgoing) {
                        SheetAction("Editar") { editing = m; replyTo = null; text = m.text; menuFor = null }
                        SheetAction("Apagar para todos") { repo.deleteForEveryone(m.id).onFailure(::toast); menuFor = null }
                    }
                }
                SheetAction("Apagar só para mim") { repo.deleteForMe(m.id); menuFor = null }
            }
        }
    }

    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = {
                    IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Voltar") }
                },
                title = {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Avatar(contact?.name ?: "?", peerId, size = 38.dp)
                        Spacer(Modifier.width(12.dp))
                        Column {
                            Text(contact?.name ?: "Contato", maxLines = 1, overflow = TextOverflow.Ellipsis, fontWeight = FontWeight.SemiBold)
                            Text(
                                when {
                                    relays > 0 && peers > 0 -> "pela internet e por perto"
                                    relays > 0 -> "pela internet"
                                    peers > 0 -> "sem internet · por perto"
                                    else -> "sem rede agora · entrega quando conectar"
                                },
                                style = MaterialTheme.typography.labelSmall,
                            )
                        }
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primary,
                    titleContentColor = MaterialTheme.colorScheme.onPrimary,
                    navigationIconContentColor = MaterialTheme.colorScheme.onPrimary,
                ),
            )
        },
        bottomBar = {
            Column(Modifier.fillMaxWidth().background(colors.wallpaper).navigationBarsPadding().imePadding()) {
                val bar = replyTo?.let { "Respondendo: " + it.text.ifEmpty { "mensagem" } } ?: editing?.let { "Editando mensagem" }
                if (bar != null) {
                    Row(
                        Modifier.fillMaxWidth().padding(start = 16.dp, end = 4.dp, top = 6.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Text(bar, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.primary, modifier = Modifier.weight(1f))
                        IconButton(onClick = {
                            if (editing != null) text = ""
                            replyTo = null
                            editing = null
                        }) { Icon(Icons.Filled.Close, contentDescription = "Cancelar") }
                    }
                }
                Row(Modifier.fillMaxWidth().padding(8.dp), verticalAlignment = Alignment.Bottom) {
                    TextField(
                        value = text,
                        onValueChange = { text = it },
                        placeholder = { Text("Mensagem") },
                        shape = RoundedCornerShape(24.dp),
                        maxLines = 5,
                        keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                        colors = TextFieldDefaults.colors(focusedIndicatorColor = Color.Transparent, unfocusedIndicatorColor = Color.Transparent),
                        modifier = Modifier.weight(1f),
                    )
                    Spacer(Modifier.width(8.dp))
                    FilledIconButton(onClick = ::submit, enabled = text.isNotBlank(), modifier = Modifier.size(52.dp)) {
                        Icon(Icons.AutoMirrored.Filled.Send, contentDescription = if (editing != null) "Salvar edição" else "Enviar")
                    }
                }
            }
        },
    ) { pad ->
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize().background(colors.wallpaper).padding(pad),
            contentPadding = PaddingValues(horizontal = 10.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            item {
                Text(
                    "Cifrado de ponta a ponta: ninguém no caminho lê, nem os servidores da internet." +
                        (contact?.let { c -> "\n" + (if (c.addedInPerson) "✓ Adicionado pessoalmente · " else "") + "código ${c.safetyCode}" } ?: ""),
                    style = MaterialTheme.typography.bodySmall,
                    textAlign = TextAlign.Center,
                    color = colors.meta,
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 24.dp, vertical = 12.dp),
                )
            }
            items(messages, key = { it.id.toString() }) { m ->
                Bubble(
                    m = m,
                    quoted = m.replyTo?.let { byId[it] },
                    onLongPress = { menuFor = m },
                    onDismissAlert = { repo.dismissAlert(m.id) },
                )
            }
        }
    }
}

@Composable
private fun SheetAction(label: String, onClick: () -> Unit) {
    Text(
        label,
        style = MaterialTheme.typography.bodyLarge,
        modifier = Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 24.dp, vertical = 14.dp),
    )
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun Bubble(m: MessageInfo, quoted: MessageInfo?, onLongPress: () -> Unit, onDismissAlert: () -> Unit) {
    val colors = LocalChatColors.current
    Column(Modifier.fillMaxWidth(), horizontalAlignment = if (m.outgoing) Alignment.End else Alignment.Start) {
        Surface(
            color = if (m.outgoing) colors.outgoing else colors.incoming,
            shape = RoundedCornerShape(
                topStart = 16.dp,
                topEnd = 16.dp,
                bottomStart = if (m.outgoing) 16.dp else 4.dp,
                bottomEnd = if (m.outgoing) 4.dp else 16.dp,
            ),
            shadowElevation = 1.dp,
            modifier = Modifier.widthIn(max = 300.dp).combinedClickable(onClick = {}, onLongClick = onLongPress, onLongClickLabel = "Opções da mensagem"),
        ) {
            Column(Modifier.padding(start = 12.dp, end = 10.dp, top = 8.dp, bottom = 6.dp)) {
                if (m.moneyAlert) {
                    Surface(color = AlertBg, shape = RoundedCornerShape(8.dp), modifier = Modifier.fillMaxWidth().padding(bottom = 6.dp)) {
                        Column(Modifier.padding(8.dp)) {
                            Text(
                                "⚠ Pedido de dinheiro ou código. Antes de pagar ou passar qualquer código, confirme pessoalmente ou por ligação.",
                                color = AlertFg,
                                style = MaterialTheme.typography.bodySmall,
                                fontWeight = FontWeight.Medium,
                            )
                            TextButton(onClick = onDismissAlert, modifier = Modifier.align(Alignment.End)) { Text("Entendi", color = AlertFg) }
                        }
                    }
                }
                if (quoted != null) {
                    Surface(
                        color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.6f),
                        shape = RoundedCornerShape(8.dp),
                        modifier = Modifier.fillMaxWidth().padding(bottom = 6.dp),
                    ) {
                        Row(Modifier.height(IntrinsicSize.Min)) {
                            Box(Modifier.width(4.dp).fillMaxHeight().background(MaterialTheme.colorScheme.primary))
                            Column(Modifier.padding(8.dp)) {
                                Text(if (quoted.outgoing) "Você" else "Contato", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.SemiBold)
                                Text(if (quoted.deleted) "Mensagem apagada" else quoted.text, maxLines = 2, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall)
                            }
                        }
                    }
                }
                if (m.deleted) {
                    Text("🚫 Mensagem apagada", fontStyle = FontStyle.Italic, color = colors.meta)
                } else {
                    Text(m.text, style = MaterialTheme.typography.bodyLarge)
                }
                Row(Modifier.align(Alignment.End), verticalAlignment = Alignment.CenterVertically) {
                    if (m.edited && !m.deleted) Text("editada · ", style = MaterialTheme.typography.labelSmall, color = colors.meta)
                    Text(clockTime(m.atMs), style = MaterialTheme.typography.labelSmall, color = colors.meta)
                    if (m.outgoing && !m.deleted) Ticks(m.status)
                }
            }
        }
        val reactions = listOfNotNull(m.peerReaction, m.myReaction)
        if (reactions.isNotEmpty() && !m.deleted) {
            Surface(
                shape = RoundedCornerShape(12.dp),
                color = MaterialTheme.colorScheme.surface,
                shadowElevation = 1.dp,
                modifier = Modifier.padding(horizontal = 8.dp).semantics { contentDescription = "Reações: ${reactions.joinToString(" ")}" },
            ) {
                Text(reactions.joinToString(" "), fontSize = 14.sp, modifier = Modifier.padding(horizontal = 8.dp, vertical = 2.dp))
            }
        }
    }
}

