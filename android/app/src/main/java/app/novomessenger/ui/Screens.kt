package app.novomessenger.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.automirrored.filled.Send
import androidx.compose.material.icons.filled.Add
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
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
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import android.widget.Toast
import app.novomessenger.Repo
import uniffi.nm_ffi.ConversationInfo
import uniffi.nm_ffi.MessageInfo

/* ---------------- Primeira abertura ---------------- */

@Composable
fun OnboardingScreen(onDone: (String) -> Unit) {
    var name by rememberSaveable { mutableStateOf("") }
    Column(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding()
            .navigationBarsPadding()
            .imePadding()
            .padding(horizontal = 28.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Text("Novo Messenger", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold, color = MaterialTheme.colorScheme.primary)
        Spacer(Modifier.height(12.dp))
        Text(
            "Converse sem internet, sem sinal e sem créditos. As mensagens vão de celular em celular e são cifradas de ponta a ponta.",
            style = MaterialTheme.typography.bodyLarge,
            textAlign = TextAlign.Center,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        Spacer(Modifier.height(32.dp))
        OutlinedTextField(
            value = name,
            onValueChange = { if (it.length <= 40) name = it },
            label = { Text("Como você quer ser chamado?") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Words),
            modifier = Modifier.fillMaxWidth(),
        )
        Spacer(Modifier.height(16.dp))
        Button(onClick = { onDone(name.trim()) }, enabled = name.isNotBlank(), modifier = Modifier.fillMaxWidth().height(52.dp)) {
            Text("Começar")
        }
        Spacer(Modifier.height(12.dp))
        Text(
            "Não pedimos número de telefone. Sua identidade é uma chave criada neste aparelho.",
            style = MaterialTheme.typography.bodySmall,
            textAlign = TextAlign.Center,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/* ---------------- Lista de conversas ---------------- */

private fun peersText(peers: Int) = when (peers) {
    0 -> "procurando aparelhos por perto…"
    1 -> "1 aparelho por perto"
    else -> "$peers aparelhos por perto"
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConversationsScreen(
    repo: Repo,
    revision: Int,
    peers: Int,
    gate: PermissionGate,
    onOpen: (String) -> Unit,
    onNewContact: () -> Unit,
) {
    val items = remember(revision) { repo.conversations() }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text("Novo Messenger", fontWeight = FontWeight.SemiBold)
                        Text(if (gate.granted) peersText(peers) else "rede sem internet desligada", style = MaterialTheme.typography.labelMedium)
                    }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primary,
                    titleContentColor = MaterialTheme.colorScheme.onPrimary,
                ),
            )
        },
        floatingActionButton = {
            FloatingActionButton(onClick = onNewContact, containerColor = MaterialTheme.colorScheme.secondary, contentColor = MaterialTheme.colorScheme.onSecondary) {
                Icon(Icons.Filled.Add, contentDescription = "Novo contato")
            }
        },
    ) { pad ->
        LazyColumn(Modifier.padding(pad).fillMaxSize()) {
            if (!gate.granted) {
                item {
                    Surface(color = MaterialTheme.colorScheme.primaryContainer, modifier = Modifier.fillMaxWidth().clickable { gate.request() }) {
                        Column(Modifier.padding(16.dp)) {
                            Text("Permita Bluetooth e dispositivos por perto", fontWeight = FontWeight.SemiBold, color = MaterialTheme.colorScheme.onPrimaryContainer)
                            Text("Sem isso o celular não encontra outros aparelhos. Toque para permitir.", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onPrimaryContainer)
                        }
                    }
                }
            }
            if (items.isEmpty()) {
                item {
                    Column(Modifier.fillMaxWidth().padding(32.dp), horizontalAlignment = Alignment.CenterHorizontally) {
                        Text("Nenhuma conversa ainda", style = MaterialTheme.typography.titleMedium)
                        Spacer(Modifier.height(8.dp))
                        Text(
                            "Toque em + e leia o QR de quem está perto de você. Depois é só escrever.",
                            textAlign = TextAlign.Center,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                }
            }
            items(items, key = { it.peerId }) { c ->
                ConversationRow(c) { onOpen(c.peerId) }
                HorizontalDivider(Modifier.padding(start = 80.dp), color = MaterialTheme.colorScheme.surfaceVariant)
            }
        }
    }
}

@Composable
private fun ConversationRow(c: ConversationInfo, onClick: () -> Unit) {
    val unread = c.unread > 0u
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 16.dp, vertical = 12.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Avatar(c.name, c.peerId)
        Spacer(Modifier.width(16.dp))
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(c.name, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                Text(
                    shortTime(c.lastAtMs),
                    style = MaterialTheme.typography.labelSmall,
                    color = if (unread) MaterialTheme.colorScheme.secondary else MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Spacer(Modifier.height(2.dp))
            Row(verticalAlignment = Alignment.CenterVertically) {
                if (c.lastOutgoing) {
                    Ticks(c.lastStatus)
                    Spacer(Modifier.width(4.dp))
                }
                Text(
                    c.lastText,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.weight(1f),
                )
                if (unread) {
                    Box(
                        Modifier.size(22.dp).background(MaterialTheme.colorScheme.secondary, CircleShape),
                        contentAlignment = Alignment.Center,
                    ) {
                        Text(c.unread.toString(), color = MaterialTheme.colorScheme.onSecondary, style = MaterialTheme.typography.labelSmall)
                    }
                }
            }
        }
    }
}

/* ---------------- Conversa ---------------- */

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ChatScreen(repo: Repo, revision: Int, peers: Int, peerId: String, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val contact = remember(revision, peerId) { repo.contact(peerId) }
    val messages = remember(revision, peerId) { repo.messages(peerId) }
    var text by rememberSaveable { mutableStateOf("") }
    val list = rememberLazyListState()
    val colors = LocalChatColors.current

    DisposableEffect(peerId) {
        repo.openChat = peerId
        onDispose { repo.openChat = null }
    }
    LaunchedEffect(revision, peerId) { repo.markRead(peerId) }
    LaunchedEffect(messages.size) { if (messages.isNotEmpty()) list.animateScrollToItem(messages.size - 1) }

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
                                if (peers > 0) "rede: ${peersText(peers)}" else "fora de alcance · entrega quando aproximar",
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
            Row(
                Modifier
                    .fillMaxWidth()
                    .background(colors.wallpaper)
                    .navigationBarsPadding()
                    .imePadding()
                    .padding(8.dp),
                verticalAlignment = Alignment.Bottom,
            ) {
                TextField(
                    value = text,
                    onValueChange = { text = it },
                    placeholder = { Text("Mensagem") },
                    shape = RoundedCornerShape(24.dp),
                    maxLines = 5,
                    keyboardOptions = KeyboardOptions(capitalization = KeyboardCapitalization.Sentences),
                    colors = TextFieldDefaults.colors(
                        focusedIndicatorColor = androidx.compose.ui.graphics.Color.Transparent,
                        unfocusedIndicatorColor = androidx.compose.ui.graphics.Color.Transparent,
                    ),
                    modifier = Modifier.weight(1f),
                )
                Spacer(Modifier.width(8.dp))
                FilledIconButton(
                    onClick = {
                        repo.send(peerId, text).fold(
                            onSuccess = { text = "" },
                            onFailure = { Toast.makeText(ctx, it.message ?: "não foi possível enviar", Toast.LENGTH_SHORT).show() },
                        )
                    },
                    enabled = text.isNotBlank(),
                    modifier = Modifier.size(52.dp),
                ) {
                    Icon(Icons.AutoMirrored.Filled.Send, contentDescription = "Enviar")
                }
            }
        },
    ) { pad ->
        LazyColumn(
            state = list,
            modifier = Modifier.fillMaxSize().background(colors.wallpaper).padding(pad),
            contentPadding = androidx.compose.foundation.layout.PaddingValues(horizontal = 10.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            if (messages.isEmpty()) {
                item {
                    Text(
                        "As mensagens são cifradas de ponta a ponta. Ninguém no caminho consegue ler.\nCódigo de segurança: ${contact?.safetyCode ?: ""}",
                        style = MaterialTheme.typography.bodySmall,
                        textAlign = TextAlign.Center,
                        color = colors.meta,
                        modifier = Modifier.fillMaxWidth().padding(24.dp),
                    )
                }
            }
            items(messages, key = { it.id.toString() }) { m -> Bubble(m) }
        }
    }
}

@Composable
private fun Bubble(m: MessageInfo) {
    val colors = LocalChatColors.current
    Box(Modifier.fillMaxWidth(), contentAlignment = if (m.outgoing) Alignment.CenterEnd else Alignment.CenterStart) {
        Surface(
            color = if (m.outgoing) colors.outgoing else colors.incoming,
            shape = RoundedCornerShape(
                topStart = 16.dp,
                topEnd = 16.dp,
                bottomStart = if (m.outgoing) 16.dp else 4.dp,
                bottomEnd = if (m.outgoing) 4.dp else 16.dp,
            ),
            shadowElevation = 1.dp,
            modifier = Modifier.widthIn(max = 300.dp),
        ) {
            Column(Modifier.padding(start = 12.dp, end = 10.dp, top = 8.dp, bottom = 6.dp)) {
                Text(m.text, style = MaterialTheme.typography.bodyLarge)
                Row(Modifier.align(Alignment.End), verticalAlignment = Alignment.CenterVertically) {
                    Text(clockTime(m.atMs), style = MaterialTheme.typography.labelSmall, color = colors.meta)
                    if (m.outgoing) Ticks(m.status)
                }
            }
        }
    }
}

/* ---------------- Novo contato ---------------- */

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewContactScreen(repo: Repo, revision: Int, onBack: () -> Unit, onScan: () -> Unit, onOpen: (String) -> Unit) {
    val contacts = remember(revision) { repo.contacts() }
    val card = remember(revision) { repo.myCard() }
    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Voltar") } },
                title = { Text("Novo contato") },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primary,
                    titleContentColor = MaterialTheme.colorScheme.onPrimary,
                    navigationIconContentColor = MaterialTheme.colorScheme.onPrimary,
                ),
            )
        },
    ) { pad ->
        Column(
            Modifier.padding(pad).fillMaxSize().verticalScroll(rememberScrollState()).padding(20.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            SectionLabel("Meu código", Modifier.align(Alignment.Start))
            Spacer(Modifier.height(12.dp))
            QrCode(card, Modifier.size(240.dp))
            Spacer(Modifier.height(12.dp))
            Text(repo.myName(), style = MaterialTheme.typography.titleMedium)
            Text("Código de segurança: ${repo.mySafetyCode()}", style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.height(8.dp))
            Text(
                "Peça para a outra pessoa abrir o Novo Messenger, tocar em + e ler este código. Depois leia o dela.",
                textAlign = TextAlign.Center,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(20.dp))
            Button(onClick = onScan, modifier = Modifier.fillMaxWidth().height(52.dp)) { Text("Ler o código de alguém") }
            Spacer(Modifier.height(28.dp))
            if (contacts.isNotEmpty()) {
                SectionLabel("Contatos", Modifier.align(Alignment.Start))
                Spacer(Modifier.height(8.dp))
                contacts.forEach { c ->
                    Row(
                        Modifier.fillMaxWidth().clickable { onOpen(c.peerId) }.padding(vertical = 10.dp),
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Avatar(c.name, c.peerId, size = 42.dp)
                        Spacer(Modifier.width(14.dp))
                        Column(Modifier.weight(1f)) {
                            Text(c.name, fontWeight = FontWeight.SemiBold)
                            Text("código ${c.safetyCode}", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                        OutlinedButton(onClick = { onOpen(c.peerId) }) { Text("Conversar") }
                    }
                }
            }
        }
    }
}
