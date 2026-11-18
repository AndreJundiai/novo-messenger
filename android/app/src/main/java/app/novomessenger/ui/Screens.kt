package app.novomessenger.ui

import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
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
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardCapitalization
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import app.novomessenger.Notifications
import app.novomessenger.Repo
import uniffi.nm_ffi.ConversationInfo

/* ---------------- Primeira abertura ---------------- */

@Composable
fun OnboardingScreen(repo: Repo, onDone: (String) -> Unit) {
    val ctx = LocalContext.current
    var name by rememberSaveable { mutableStateOf("") }
    var backup by remember { mutableStateOf<ByteArray?>(null) }
    val pick = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) backup = readUri(ctx, uri)
    }
    backup?.let { data ->
        PasswordDialog(
            title = "Restaurar backup",
            explanation = "Digite a senha que você escolheu ao criar o backup.",
            confirm = "Restaurar",
            onDismiss = { backup = null },
            onConfirm = { pwd ->
                repo.restoreBackup(data, pwd).fold(
                    onSuccess = { Toast.makeText(ctx, "Conversas restauradas", Toast.LENGTH_SHORT).show() },
                    onFailure = { Toast.makeText(ctx, it.message ?: "não foi possível restaurar", Toast.LENGTH_LONG).show() },
                )
                backup = null
            },
        )
    }
    Column(
        Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .statusBarsPadding()
            .navigationBarsPadding()
            .imePadding()
            .verticalScroll(rememberScrollState())
            .padding(horizontal = 28.dp, vertical = 24.dp),
        verticalArrangement = Arrangement.Center,
        horizontalAlignment = Alignment.CenterHorizontally,
    ) {
        Spacer(Modifier.height(48.dp))
        Text("Novo Messenger", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.SemiBold, color = MaterialTheme.colorScheme.primary)
        Spacer(Modifier.height(12.dp))
        Text(
            "Converse pela internet ou sem ela: sem sinal, as mensagens vão de celular em celular. Tudo cifrado de ponta a ponta.",
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
        Spacer(Modifier.height(8.dp))
        TextButton(onClick = { pick.launch(arrayOf("*/*")) }) { Text("Já tenho um backup: restaurar") }
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ConversationsScreen(
    repo: Repo,
    revision: Int,
    peers: Int,
    relays: Int,
    gate: PermissionGate,
    onOpen: (String) -> Unit,
    onNewContact: () -> Unit,
    onSearch: () -> Unit,
    onSettings: () -> Unit,
) {
    val items = remember(revision) { repo.conversations() }
    Scaffold(
        topBar = {
            TopAppBar(
                title = {
                    Column {
                        Text("Novo Messenger", fontWeight = FontWeight.SemiBold)
                        Text(Notifications.networkSummary(peers, relays), style = MaterialTheme.typography.labelMedium)
                    }
                },
                actions = {
                    IconButton(onClick = onSearch) { Icon(Icons.Filled.Search, contentDescription = "Buscar mensagens") }
                    IconButton(onClick = onSettings) { Icon(Icons.Filled.Settings, contentDescription = "Configurações") }
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primary,
                    titleContentColor = MaterialTheme.colorScheme.onPrimary,
                    actionIconContentColor = MaterialTheme.colorScheme.onPrimary,
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
                            Text(
                                "Pela internet já funciona. Sem essa permissão o celular não conversa com quem está perto quando não há sinal. Toque para permitir.",
                                style = MaterialTheme.typography.bodySmall,
                                color = MaterialTheme.colorScheme.onPrimaryContainer,
                            )
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
                            "Toque em + e leia o QR da outra pessoa, pessoalmente. Depois é só escrever, com ou sem internet.",
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
                if (c.lastOutgoing && !c.lastDeleted) {
                    Ticks(c.lastStatus)
                    Spacer(Modifier.width(4.dp))
                }
                Text(
                    if (c.lastDeleted) "Mensagem apagada" else c.lastText,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    fontStyle = if (c.lastDeleted) FontStyle.Italic else FontStyle.Normal,
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

/* ---------------- Novo contato ---------------- */

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun NewContactScreen(repo: Repo, revision: Int, onBack: () -> Unit, onScan: () -> Unit, onPhoneBook: () -> Unit, onOpen: (String) -> Unit) {
    val ctx = LocalContext.current
    val contacts = remember(revision) { repo.contacts() }
    val card = remember(revision) { repo.myCard() }
    var pasted by rememberSaveable { mutableStateOf("") }
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
                "Mostre este código pessoalmente. Contatos só entram assim, e isso protege contra o golpe do \"troquei de número\".",
                textAlign = TextAlign.Center,
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Spacer(Modifier.height(20.dp))
            Button(onClick = onScan, modifier = Modifier.fillMaxWidth().height(52.dp)) { Text("Ler o código de alguém") }
            Spacer(Modifier.height(10.dp))
            OutlinedButton(onClick = onPhoneBook, modifier = Modifier.fillMaxWidth().height(52.dp)) { Text("Contatos do celular") }
            Spacer(Modifier.height(20.dp))
            SectionLabel("Colar código", Modifier.align(Alignment.Start))
            Spacer(Modifier.height(6.dp))
            OutlinedTextField(
                value = pasted,
                onValueChange = { pasted = it.trim() },
                label = { Text("Código recebido por SMS ou mensagem") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            TextButton(
                onClick = {
                    repo.acceptCode(pasted).fold(
                        onSuccess = {
                            pasted = ""
                            Toast.makeText(ctx, "${it.name} adicionado (não conferido pessoalmente)", Toast.LENGTH_LONG).show()
                            onOpen(it.peerId)
                        },
                        onFailure = { Toast.makeText(ctx, it.message ?: "código inválido", Toast.LENGTH_LONG).show() },
                    )
                },
                enabled = pasted.isNotBlank(),
                modifier = Modifier.align(Alignment.End),
            ) { Text("Adicionar pelo código") }
            Spacer(Modifier.height(20.dp))
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
                            Text(
                                contactBadge(c) + " · código ${c.safetyCode}",
                                style = MaterialTheme.typography.labelSmall,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                        OutlinedButton(onClick = { onOpen(c.peerId) }) { Text("Conversar") }
                    }
                }
            }
        }
    }
}

/** "✓ adicionado pessoalmente", "✓ número confirmado por SMS" ou "não conferido". */
fun contactBadge(c: uniffi.nm_ffi.ContactInfo): String = when {
    c.addedInPerson -> "✓ adicionado pessoalmente"
    c.phoneVerified -> "✓ número confirmado por SMS"
    c.phone != null -> "aguardando confirmação por SMS"
    else -> "não conferido pessoalmente"
}
