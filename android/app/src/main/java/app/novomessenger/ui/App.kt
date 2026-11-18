package app.novomessenger.ui

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.novomessenger.Repo
import uniffi.nm_ffi.ContactInfo
import uniffi.nm_ffi.cardName

sealed interface Screen {
    data object Conversations : Screen
    data object NewContact : Screen
    data object Scan : Screen
    data object Search : Screen
    data object Settings : Screen
    data object PhoneContacts : Screen
    data class Chat(val peerId: String) : Screen
}

/** Permissões sem as quais a conexão por perto (sem internet) não funciona. */
private fun meshPermissions(): List<String> = buildList {
    if (Build.VERSION.SDK_INT >= 31) {
        add(Manifest.permission.BLUETOOTH_SCAN)
        add(Manifest.permission.BLUETOOTH_ADVERTISE)
        add(Manifest.permission.BLUETOOTH_CONNECT)
    }
    if (Build.VERSION.SDK_INT <= 32) add(Manifest.permission.ACCESS_FINE_LOCATION)
    if (Build.VERSION.SDK_INT >= 33) add(Manifest.permission.NEARBY_WIFI_DEVICES)
}

private fun allGranted(ctx: Context, perms: List<String>) =
    perms.all { ContextCompat.checkSelfPermission(ctx, it) == PackageManager.PERMISSION_GRANTED }

class PermissionGate(val granted: Boolean, val request: () -> Unit)

@Composable
private fun rememberMeshPermissions(onResult: () -> Unit): PermissionGate {
    val ctx = LocalContext.current
    val needed = remember { meshPermissions() }
    val ask = remember {
        (needed + if (Build.VERSION.SDK_INT >= 33) listOf(Manifest.permission.POST_NOTIFICATIONS) else emptyList()).toTypedArray()
    }
    var granted by remember { mutableStateOf(allGranted(ctx, needed)) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        granted = allGranted(ctx, needed)
        onResult()
    }
    LaunchedEffect(Unit) {
        // A internet funciona mesmo sem Bluetooth: a rede liga já, e a parte "por perto" entra quando a permissão vier.
        onResult()
        if (!granted) launcher.launch(ask)
    }
    return PermissionGate(granted) { launcher.launch(ask) }
}

@Composable
fun App(repo: Repo, onMeshAllowed: () -> Unit) {
    val revision by repo.revision.collectAsStateWithLifecycle()
    val peers by repo.peers.collectAsStateWithLifecycle()
    val relays by repo.relays.collectAsStateWithLifecycle()
    val ctx = LocalContext.current

    if (!repo.hasIdentity()) {
        OnboardingScreen(repo = repo, onDone = { repo.createIdentity(it) })
        return
    }

    val gate = rememberMeshPermissions(onMeshAllowed)
    var screen by remember { mutableStateOf<Screen>(Screen.Conversations) }
    // Código lido que tem nome parecido com um contato: espera a pessoa confirmar.
    var pendingCard by remember { mutableStateOf<Pair<String, List<ContactInfo>>?>(null) }

    BackHandler(enabled = screen != Screen.Conversations) {
        screen = if (screen == Screen.Scan || screen == Screen.PhoneContacts) Screen.NewContact else Screen.Conversations
    }

    fun add(card: String) {
        repo.addContact(card).fold(
            onSuccess = {
                Toast.makeText(ctx, "${it.name} adicionado pessoalmente ✓", Toast.LENGTH_SHORT).show()
                screen = Screen.Chat(it.peerId)
            },
            onFailure = {
                Toast.makeText(ctx, it.message ?: "código inválido", Toast.LENGTH_LONG).show()
                screen = Screen.NewContact
            },
        )
    }

    pendingCard?.let { (card, similar) ->
        val newName = cardName(card) ?: "essa pessoa"
        AlertDialog(
            onDismissRequest = { pendingCard = null; screen = Screen.NewContact },
            title = { Text("Atenção: nome parecido") },
            text = {
                Text(
                    "Você já tem ${similar.joinToString { "\"${it.name}\"" }} nos contatos, e \"$newName\" é outra pessoa, " +
                        "com outro código de segurança.\n\nSe alguém disse que trocou de número, confirme pessoalmente " +
                        "ou por ligação antes de conversar. É assim que funciona o golpe do número novo.",
                )
            },
            confirmButton = { TextButton(onClick = { pendingCard = null; add(card) }) { Text("Adicionar mesmo assim") } },
            dismissButton = { TextButton(onClick = { pendingCard = null; screen = Screen.NewContact }) { Text("Cancelar") } },
        )
    }

    when (val s = screen) {
        Screen.Conversations -> ConversationsScreen(
            repo = repo,
            revision = revision,
            peers = peers,
            relays = relays,
            gate = gate,
            onOpen = { screen = Screen.Chat(it) },
            onNewContact = { screen = Screen.NewContact },
            onSearch = { screen = Screen.Search },
            onSettings = { screen = Screen.Settings },
        )
        Screen.NewContact -> NewContactScreen(
            repo = repo,
            revision = revision,
            onBack = { screen = Screen.Conversations },
            onScan = { screen = Screen.Scan },
            onPhoneBook = { screen = Screen.PhoneContacts },
            onOpen = { screen = Screen.Chat(it) },
        )
        Screen.PhoneContacts -> PhoneContactsScreen(
            repo = repo,
            revision = revision,
            relays = relays,
            onBack = { screen = Screen.NewContact },
            onOpen = { screen = Screen.Chat(it) },
        )
        Screen.Scan -> ScanScreen(
            onBack = { screen = Screen.NewContact },
            onCode = { code ->
                val similar = repo.similarContacts(code)
                if (similar.isEmpty()) add(code) else pendingCard = code to similar
            },
        )
        Screen.Search -> SearchScreen(
            repo = repo,
            revision = revision,
            onBack = { screen = Screen.Conversations },
            onOpen = { screen = Screen.Chat(it) },
        )
        Screen.Settings -> SettingsScreen(repo = repo, revision = revision, onBack = { screen = Screen.Conversations })
        is Screen.Chat -> ChatScreen(
            repo = repo,
            revision = revision,
            peers = peers,
            relays = relays,
            peerId = s.peerId,
            onBack = { screen = Screen.Conversations },
        )
    }
}
