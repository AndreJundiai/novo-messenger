package app.novomessenger.ui

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import android.widget.Toast
import androidx.activity.compose.BackHandler
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
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

sealed interface Screen {
    data object Conversations : Screen
    data object NewContact : Screen
    data object Scan : Screen
    data class Chat(val peerId: String) : Screen
}

/** Permissões sem as quais a rede sem internet não funciona. */
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
private fun rememberMeshPermissions(onGranted: () -> Unit): PermissionGate {
    val ctx = LocalContext.current
    val needed = remember { meshPermissions() }
    val ask = remember {
        (needed + if (Build.VERSION.SDK_INT >= 33) listOf(Manifest.permission.POST_NOTIFICATIONS) else emptyList()).toTypedArray()
    }
    var granted by remember { mutableStateOf(allGranted(ctx, needed)) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) {
        granted = allGranted(ctx, needed)
        if (granted) onGranted()
    }
    LaunchedEffect(Unit) {
        if (granted) onGranted() else launcher.launch(ask)
    }
    return PermissionGate(granted) { launcher.launch(ask) }
}

@Composable
fun App(repo: Repo, onMeshAllowed: () -> Unit) {
    val revision by repo.revision.collectAsStateWithLifecycle()
    val peers by repo.peers.collectAsStateWithLifecycle()
    val ctx = LocalContext.current

    if (!repo.hasIdentity()) {
        OnboardingScreen(onDone = { repo.createIdentity(it) })
        return
    }

    val gate = rememberMeshPermissions(onMeshAllowed)
    var screen by remember { mutableStateOf<Screen>(Screen.Conversations) }
    BackHandler(enabled = screen != Screen.Conversations) {
        screen = if (screen == Screen.Scan) Screen.NewContact else Screen.Conversations
    }

    when (val s = screen) {
        Screen.Conversations -> ConversationsScreen(
            repo = repo,
            revision = revision,
            peers = peers,
            gate = gate,
            onOpen = { screen = Screen.Chat(it) },
            onNewContact = { screen = Screen.NewContact },
        )
        Screen.NewContact -> NewContactScreen(
            repo = repo,
            revision = revision,
            onBack = { screen = Screen.Conversations },
            onScan = { screen = Screen.Scan },
            onOpen = { screen = Screen.Chat(it) },
        )
        Screen.Scan -> ScanScreen(
            onBack = { screen = Screen.NewContact },
            onCode = { code ->
                repo.addContact(code).fold(
                    onSuccess = {
                        Toast.makeText(ctx, "${it.name} adicionado", Toast.LENGTH_SHORT).show()
                        screen = Screen.Chat(it.peerId)
                    },
                    onFailure = {
                        Toast.makeText(ctx, it.message ?: "código inválido", Toast.LENGTH_LONG).show()
                        screen = Screen.NewContact
                    },
                )
            },
        )
        is Screen.Chat -> ChatScreen(
            repo = repo,
            revision = revision,
            peers = peers,
            peerId = s.peerId,
            onBack = { screen = Screen.Conversations },
        )
    }
}
