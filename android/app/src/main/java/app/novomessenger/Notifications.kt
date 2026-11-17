package app.novomessenger

import android.Manifest
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.ContextCompat

object Notifications {
    private const val CH_MESSAGES = "messages"
    private const val CH_MESH = "mesh"
    const val MESH_ID = 1

    fun ensureChannels(ctx: Context) {
        val nm = ctx.getSystemService(NotificationManager::class.java)
        nm.createNotificationChannel(NotificationChannel(CH_MESSAGES, "Mensagens", NotificationManager.IMPORTANCE_HIGH))
        nm.createNotificationChannel(
            NotificationChannel(CH_MESH, "Rede sem internet", NotificationManager.IMPORTANCE_LOW).apply {
                description = "Mantém o celular procurando contatos por perto"
            },
        )
    }

    private fun openApp(ctx: Context): PendingIntent =
        PendingIntent.getActivity(
            ctx,
            0,
            Intent(ctx, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT,
        )

    fun mesh(ctx: Context, peers: Int): Notification =
        NotificationCompat.Builder(ctx, CH_MESH)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle("Novo Messenger")
            .setContentText(
                when (peers) {
                    0 -> "Procurando aparelhos por perto…"
                    1 -> "Conectado a 1 aparelho"
                    else -> "Conectado a $peers aparelhos"
                },
            )
            .setOngoing(true)
            .setSilent(true)
            .setContentIntent(openApp(ctx))
            .build()

    fun message(ctx: Context, peerId: String, name: String, text: String) {
        if (Build.VERSION.SDK_INT >= 33 &&
            ContextCompat.checkSelfPermission(ctx, Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED
        ) {
            return
        }
        val n = NotificationCompat.Builder(ctx, CH_MESSAGES)
            .setSmallIcon(R.drawable.ic_stat_mesh)
            .setContentTitle(name)
            .setContentText(text)
            .setStyle(NotificationCompat.BigTextStyle().bigText(text))
            .setAutoCancel(true)
            .setContentIntent(openApp(ctx))
            .build()
        try {
            NotificationManagerCompat.from(ctx).notify(peerId.hashCode(), n)
        } catch (_: SecurityException) {
        }
    }
}
