package app.novomessenger

import android.app.Application
import android.content.Intent
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.core.content.ContextCompat
import app.novomessenger.ui.App
import app.novomessenger.ui.NmTheme

class NmApplication : Application() {
    lateinit var repo: Repo
        private set

    override fun onCreate() {
        super.onCreate()
        repo = Repo(this)
        Notifications.ensureChannels(this)
    }
}

class MainActivity : ComponentActivity() {
    private val repo get() = (application as NmApplication).repo

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            NmTheme {
                App(repo = repo, onMeshAllowed = ::startMesh)
            }
        }
    }

    /** Liga a rede sem internet (só depois que as permissões foram dadas). */
    private fun startMesh() {
        if (!repo.hasIdentity()) return
        ContextCompat.startForegroundService(this, Intent(this, MeshService::class.java))
    }

    override fun onResume() {
        super.onResume()
        repo.foreground = true
    }

    override fun onPause() {
        repo.foreground = false
        super.onPause()
    }
}
