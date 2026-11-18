package app.novomessenger.ui

import android.Manifest
import android.content.ActivityNotFoundException
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.ContactsContract
import android.widget.Toast
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TextField
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.novomessenger.Repo
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

/** Um número da agenda do celular. */
data class PhoneEntry(val name: String, val raw: String, val e164: String)

/** Lê a agenda (só no aparelho; nada sai daqui além dos resumos dos números). */
private fun readPhoneBook(ctx: Context): List<PhoneEntry> {
    val out = LinkedHashMap<String, PhoneEntry>()
    val projection = arrayOf(ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME, ContactsContract.CommonDataKinds.Phone.NUMBER)
    ctx.contentResolver.query(
        ContactsContract.CommonDataKinds.Phone.CONTENT_URI,
        projection,
        null,
        null,
        ContactsContract.CommonDataKinds.Phone.DISPLAY_NAME + " ASC",
    )?.use { c ->
        while (c.moveToNext()) {
            val name = c.getString(0) ?: continue
            val raw = c.getString(1) ?: continue
            val e164 = uniffi.nm_ffi.normalizePhone(raw) ?: continue
            out.putIfAbsent(e164, PhoneEntry(name, raw, e164))
        }
    }
    return out.values.toList()
}

/** Abre o app de SMS do celular com o texto pronto; a pessoa confirma o envio. */
fun openSms(ctx: Context, number: String, body: String) {
    val intent = Intent(Intent.ACTION_SENDTO, Uri.parse("smsto:" + Uri.encode(number))).putExtra("sms_body", body)
    try {
        ctx.startActivity(intent)
    } catch (_: ActivityNotFoundException) {
        Toast.makeText(ctx, "Nenhum app de SMS encontrado", Toast.LENGTH_LONG).show()
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun PhoneContactsScreen(repo: Repo, revision: Int, relays: Int, onBack: () -> Unit, onOpen: (String) -> Unit) {
    val ctx = LocalContext.current
    var granted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(ctx, Manifest.permission.READ_CONTACTS) == PackageManager.PERMISSION_GRANTED)
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted = it }
    var entries by remember { mutableStateOf<List<PhoneEntry>>(emptyList()) }
    var hashes by remember { mutableStateOf<Map<String, String>>(emptyMap()) }
    var progress by remember { mutableIntStateOf(0) }
    var filter by rememberSaveable { mutableStateOf("") }
    var smsFor by remember { mutableStateOf<PhoneEntry?>(null) }
    val directory by repo.directory.collectAsStateWithLifecycle()
    val appContacts = remember(revision) { repo.contacts() }
    val byPhone = remember(appContacts) { appContacts.filter { it.phone != null }.associateBy { it.phone!! } }

    LaunchedEffect(granted) {
        if (!granted) {
            ask.launch(Manifest.permission.READ_CONTACTS)
            return@LaunchedEffect
        }
        entries = withContext(Dispatchers.IO) { readPhoneBook(ctx) }
        // O resumo de cada número é lento de propósito (protege o diretório); calcula em segundo plano.
        val computed = HashMap<String, String>()
        for ((i, e) in entries.withIndex()) {
            computed[e.e164] = withContext(Dispatchers.Default) { repo.phoneHash(e.e164) }
            progress = i + 1
            if ((i + 1) % 25 == 0 || i == entries.lastIndex) {
                hashes = HashMap(computed)
                repo.queryDirectory(computed.values.toList())
            }
        }
    }

    smsFor?.let { e ->
        AlertDialog(
            onDismissRequest = { smsFor = null },
            title = { Text("Mandar por SMS") },
            text = { Text("${e.name} não tem o Novo Messenger. O SMS sai pelo app de mensagens do celular, NÃO é cifrado e pode gastar créditos da operadora.") },
            confirmButton = {
                TextButton(onClick = { openSms(ctx, e.raw, ""); smsFor = null }) { Text("Escrever SMS") }
            },
            dismissButton = {
                TextButton(onClick = { openSms(ctx, e.raw, repo.inviteSms()); smsFor = null }) { Text("Mandar convite") }
            },
        )
    }

    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = { IconButton(onClick = onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Voltar") } },
                title = { TextField(value = filter, onValueChange = { filter = it }, placeholder = { Text("Contatos do celular") }, singleLine = true) },
                colors = TopAppBarDefaults.topAppBarColors(
                    containerColor = MaterialTheme.colorScheme.primary,
                    navigationIconContentColor = MaterialTheme.colorScheme.onPrimary,
                ),
            )
        },
    ) { pad ->
        if (!granted) {
            Column(Modifier.padding(pad).fillMaxSize().padding(32.dp), verticalArrangement = Arrangement.Center, horizontalAlignment = Alignment.CenterHorizontally) {
                Text("Para mostrar quais contatos têm o app, o Novo Messenger precisa ler a agenda. Ela é lida só neste celular.")
                Button(onClick = { ask.launch(Manifest.permission.READ_CONTACTS) }, modifier = Modifier.padding(top = 16.dp)) { Text("Permitir agenda") }
            }
            return@Scaffold
        }
        val shown = remember(entries, filter) {
            if (filter.isBlank()) entries else entries.filter { it.name.contains(filter, ignoreCase = true) || it.raw.contains(filter) }
        }
        LazyColumn(Modifier.padding(pad).fillMaxSize()) {
            item {
                Surface(color = MaterialTheme.colorScheme.primaryContainer, modifier = Modifier.fillMaxWidth()) {
                    Column(Modifier.padding(16.dp)) {
                        Text(
                            if (relays == 0) "Sem internet: não dá para saber quem tem o app agora. Convites por SMS funcionam."
                            else "Só aparece quem ativou \"Deixar quem tem meu número me encontrar\". Adicionar pelo número manda um SMS de confirmação.",
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onPrimaryContainer,
                        )
                        if (entries.isNotEmpty() && progress < entries.size) {
                            Spacer(Modifier.width(4.dp))
                            LinearProgressIndicator(progress = { progress.toFloat() / entries.size }, modifier = Modifier.fillMaxWidth().padding(top = 8.dp))
                            Text("Procurando quem tem o app… $progress de ${entries.size}", style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onPrimaryContainer)
                        }
                    }
                }
            }
            items(shown, key = { it.e164 }) { e ->
                val linked = byPhone[e.e164]
                val card = hashes[e.e164]?.let { directory[it] }
                Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                    Avatar(e.name, e.e164, size = 42.dp)
                    Spacer(Modifier.width(14.dp))
                    Column(Modifier.weight(1f)) {
                        Text(e.name, fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(
                            when {
                                linked?.phoneVerified == true -> "✓ No Novo Messenger · número confirmado"
                                linked != null -> "No Novo Messenger · aguardando confirmação por SMS"
                                card != null -> "Tem o Novo Messenger"
                                else -> e.raw
                            },
                            style = MaterialTheme.typography.labelSmall,
                            color = if (linked != null || card != null) MaterialTheme.colorScheme.secondary else MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    when {
                        linked != null -> OutlinedButton(onClick = { onOpen(linked.peerId) }) { Text("Conversar") }
                        card != null -> Button(onClick = {
                            repo.addFoundContact(card, e.e164).fold(
                                onSuccess = { c ->
                                    repo.verificationSms(c.peerId).onSuccess { openSms(ctx, e.raw, it) }
                                    Toast.makeText(ctx, "Confirme o envio do SMS: ele prova que o número é mesmo de ${e.name}", Toast.LENGTH_LONG).show()
                                    onOpen(c.peerId)
                                },
                                onFailure = { Toast.makeText(ctx, it.message ?: "não foi possível", Toast.LENGTH_LONG).show() },
                            )
                        }) { Text("Adicionar") }
                        else -> OutlinedButton(onClick = { smsFor = e }) { Text("SMS") }
                    }
                }
                HorizontalDivider(Modifier.padding(start = 72.dp))
            }
        }
    }
}
