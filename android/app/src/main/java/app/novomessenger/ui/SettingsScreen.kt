package app.novomessenger.ui

import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
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
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.novomessenger.Repo
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale

/** Lê todo o conteúdo de um arquivo escolhido pela pessoa. */
fun readUri(ctx: Context, uri: Uri): ByteArray? = try {
    ctx.contentResolver.openInputStream(uri)?.use { it.readBytes() }
} catch (e: Exception) {
    null
}

private fun writeUri(ctx: Context, uri: Uri, data: ByteArray): Boolean = try {
    ctx.contentResolver.openOutputStream(uri, "w")?.use { it.write(data) } != null
} catch (e: Exception) {
    false
}

@Composable
fun PasswordDialog(
    title: String,
    explanation: String,
    confirm: String,
    askTwice: Boolean = false,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    var pwd by remember { mutableStateOf("") }
    var again by remember { mutableStateOf("") }
    val ok = pwd.length >= 6 && (!askTwice || pwd == again)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            Column {
                Text(explanation, style = MaterialTheme.typography.bodyMedium)
                Spacer(Modifier.height(12.dp))
                OutlinedTextField(
                    value = pwd,
                    onValueChange = { pwd = it },
                    label = { Text("Senha (mínimo 6 caracteres)") },
                    singleLine = true,
                    visualTransformation = PasswordVisualTransformation(),
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                )
                if (askTwice) {
                    Spacer(Modifier.height(8.dp))
                    OutlinedTextField(
                        value = again,
                        onValueChange = { again = it },
                        label = { Text("Repita a senha") },
                        singleLine = true,
                        isError = again.isNotEmpty() && again != pwd,
                        visualTransformation = PasswordVisualTransformation(),
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                    )
                }
            }
        },
        confirmButton = { TextButton(onClick = { onConfirm(pwd) }, enabled = ok) { Text(confirm) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Cancelar") } },
    )
}

@Composable
private fun SettingRow(title: String, subtitle: String, checked: Boolean? = null, onClick: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onClick).padding(horizontal = 20.dp, vertical = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.bodyLarge)
            Text(subtitle, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (checked != null) {
            Spacer(Modifier.width(12.dp))
            Switch(checked = checked, onCheckedChange = { onClick() })
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SettingsScreen(repo: Repo, revision: Int, onBack: () -> Unit) {
    val ctx = LocalContext.current
    val internet by repo.internetEnabled.collectAsStateWithLifecycle()
    val alerts = remember(revision) { repo.moneyAlerts() }
    var askExport by remember { mutableStateOf(false) }
    var exportData by remember { mutableStateOf<ByteArray?>(null) }
    var restoreData by remember { mutableStateOf<ByteArray?>(null) }
    var editingName by rememberSaveable { mutableStateOf<String?>(null) }
    var editingPhone by rememberSaveable { mutableStateOf<String?>(null) }
    val discoverable = remember(revision) { repo.discoverable() }
    val myPhone = remember(revision, editingPhone) { repo.myPhone() }

    val create = rememberLauncherForActivityResult(ActivityResultContracts.CreateDocument("application/octet-stream")) { uri ->
        val data = exportData
        exportData = null
        if (uri != null && data != null) {
            val ok = writeUri(ctx, uri, data)
            Toast.makeText(ctx, if (ok) "Backup salvo. Guarde a senha: sem ela o arquivo não abre." else "Não foi possível salvar o arquivo", Toast.LENGTH_LONG).show()
        }
    }
    val open = rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
        if (uri != null) restoreData = readUri(ctx, uri)
    }

    if (askExport) {
        PasswordDialog(
            title = "Criar backup cifrado",
            explanation = "O arquivo leva suas conversas, contatos e sua identidade, cifrados com esta senha. Se esquecer a senha, não há como recuperar.",
            confirm = "Criar",
            askTwice = true,
            onDismiss = { askExport = false },
            onConfirm = { pwd ->
                askExport = false
                repo.exportBackup(pwd).fold(
                    onSuccess = {
                        exportData = it
                        val day = SimpleDateFormat("yyyy-MM-dd", Locale.US).format(Date())
                        create.launch("novo-messenger-$day.nmbk")
                    },
                    onFailure = { Toast.makeText(ctx, it.message ?: "não foi possível", Toast.LENGTH_LONG).show() },
                )
            },
        )
    }
    restoreData?.let { data ->
        PasswordDialog(
            title = "Restaurar backup",
            explanation = "Isto troca TUDO deste aparelho pelo conteúdo do backup: conversas, contatos e identidade.",
            confirm = "Restaurar",
            onDismiss = { restoreData = null },
            onConfirm = { pwd ->
                restoreData = null
                repo.restoreBackup(data, pwd).fold(
                    onSuccess = { Toast.makeText(ctx, "Conversas restauradas", Toast.LENGTH_SHORT).show() },
                    onFailure = { Toast.makeText(ctx, it.message ?: "não foi possível restaurar", Toast.LENGTH_LONG).show() },
                )
            },
        )
    }
    editingName?.let { current ->
        var name by remember { mutableStateOf(current) }
        AlertDialog(
            onDismissRequest = { editingName = null },
            title = { Text("Seu nome") },
            text = { OutlinedTextField(value = name, onValueChange = { if (it.length <= 40) name = it }, singleLine = true) },
            confirmButton = {
                TextButton(onClick = { repo.setMyName(name.trim()); editingName = null }, enabled = name.isNotBlank()) { Text("Salvar") }
            },
            dismissButton = { TextButton(onClick = { editingName = null }) { Text("Cancelar") } },
        )
    }

    editingPhone?.let { current ->
        var number by remember { mutableStateOf(current) }
        AlertDialog(
            onDismissRequest = { editingPhone = null },
            title = { Text("Meu número") },
            text = {
                Column {
                    Text("Opcional. Fica só neste aparelho; no diretório vai apenas um resumo embaralhado.", style = MaterialTheme.typography.bodySmall)
                    OutlinedTextField(
                        value = number,
                        onValueChange = { number = it },
                        singleLine = true,
                        placeholder = { Text("(11) 98765-4321") },
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Phone),
                    )
                }
            },
            confirmButton = {
                TextButton(onClick = {
                    if (repo.setMyPhone(number)) editingPhone = null
                    else Toast.makeText(ctx, "Esse número não parece válido", Toast.LENGTH_SHORT).show()
                }) { Text("Salvar") }
            },
            dismissButton = { TextButton(onClick = { editingPhone = null }) { Text("Cancelar") } },
        )
    }

    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Voltar") } },
                title = { Text("Configurações") },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primary,
                    titleContentColor = MaterialTheme.colorScheme.onPrimary,
                    navigationIconContentColor = MaterialTheme.colorScheme.onPrimary,
                ),
            )
        },
    ) { pad ->
        Column(Modifier.padding(pad).fillMaxSize().verticalScroll(rememberScrollState())) {
            SectionLabel("Você", Modifier.padding(start = 20.dp, top = 20.dp, bottom = 4.dp))
            SettingRow(repo.myName(), "Toque para mudar · código de segurança ${repo.mySafetyCode()}") { editingName = repo.myName() }
            SettingRow("Meu número", if (myPhone.isEmpty()) "Opcional · não informado" else "+$myPhone") { editingPhone = myPhone }
            SettingRow(
                "Deixar quem tem meu número me encontrar",
                "Quem tem seu número salvo vê que você tem o app. Vai só um resumo embaralhado do número, mas números podem ser adivinhados com muito esforço: ative só se quiser.",
                checked = discoverable,
            ) {
                if (myPhone.isEmpty() && !discoverable) editingPhone = "" else repo.setDiscoverable(!discoverable)
            }
            HorizontalDivider()

            SectionLabel("Rede", Modifier.padding(start = 20.dp, top = 20.dp, bottom = 4.dp))
            SettingRow(
                "Usar a internet",
                "Quando houver conexão, as mensagens também vão por servidores públicos Nostr, que só veem dados cifrados.",
                checked = internet,
            ) { repo.setInternetEnabled(!internet) }
            SettingRow("Não deixar o celular matar o app", "Abre o ajuste de bateria. Escolha \"Não otimizar\" para o Novo Messenger, ou as mensagens podem atrasar.") {
                try {
                    ctx.startActivity(Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS))
                } catch (_: ActivityNotFoundException) {
                    Toast.makeText(ctx, "Abra Configurações → Bateria e libere o Novo Messenger", Toast.LENGTH_LONG).show()
                }
            }
            HorizontalDivider()

            SectionLabel("Segurança", Modifier.padding(start = 20.dp, top = 20.dp, bottom = 4.dp))
            SettingRow(
                "Avisar sobre pedidos de dinheiro",
                "Mostra um alerta quando uma mensagem fala de Pix, transferência ou código. A análise é feita só neste aparelho.",
                checked = alerts,
            ) { repo.setMoneyAlerts(!alerts) }
            HorizontalDivider()

            SectionLabel("Backup", Modifier.padding(start = 20.dp, top = 20.dp, bottom = 4.dp))
            SettingRow("Criar backup cifrado", "Um arquivo com senha, sem nuvem. Guarde no cartão, no computador ou envie para outro celular.") { askExport = true }
            SettingRow("Restaurar backup", "Troca tudo deste aparelho pelo conteúdo de um backup.") { open.launch(arrayOf("*/*")) }
            Spacer(Modifier.height(24.dp))
        }
    }
}

/* ---------------- Busca ---------------- */

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SearchScreen(repo: Repo, revision: Int, onBack: () -> Unit, onOpen: (String) -> Unit) {
    var query by rememberSaveable { mutableStateOf("") }
    val results = remember(query, revision) { if (query.isBlank()) emptyList() else repo.search(query) }
    val names = remember(revision) { repo.contacts().associate { it.peerId to it.name } }
    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Voltar") } },
                title = {
                    TextField(
                        value = query,
                        onValueChange = { query = it },
                        placeholder = { Text("Buscar nas conversas") },
                        singleLine = true,
                        modifier = Modifier.fillMaxWidth(),
                    )
                },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primary,
                    navigationIconContentColor = MaterialTheme.colorScheme.onPrimary,
                ),
            )
        },
    ) { pad ->
        LazyColumn(Modifier.padding(pad).fillMaxSize()) {
            if (query.isNotBlank() && results.isEmpty()) {
                item { Text("Nada encontrado", modifier = Modifier.padding(24.dp), color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
            items(results, key = { it.id.toString() }) { m ->
                val name = names[m.peerId] ?: "Contato"
                Row(
                    Modifier.fillMaxWidth().clickable { onOpen(m.peerId) }.padding(horizontal = 16.dp, vertical = 12.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Avatar(name, m.peerId, size = 40.dp)
                    Spacer(Modifier.width(14.dp))
                    Column(Modifier.weight(1f)) {
                        Row {
                            Text(if (m.outgoing) "Você → $name" else name, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
                            Text(shortTime(m.atMs), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                        }
                        Text(m.text, maxLines = 2, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                HorizontalDivider(Modifier.padding(start = 70.dp))
            }
        }
    }
}
