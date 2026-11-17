package app.novomessenger.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.Immutable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.graphics.Color

/** Cores próprias da conversa, nos dois temas. */
@Immutable
data class ChatColors(
    val outgoing: Color,
    val incoming: Color,
    val wallpaper: Color,
    val tick: Color,
    val tickRead: Color,
    val meta: Color,
)

private val LightChat = ChatColors(
    outgoing = Color(0xFFD3F0EA),
    incoming = Color(0xFFFFFFFF),
    wallpaper = Color(0xFFEEF2EF),
    tick = Color(0xFF7A8A86),
    tickRead = Color(0xFF1E88E5),
    meta = Color(0xFF6B7B77),
)

private val DarkChat = ChatColors(
    outgoing = Color(0xFF0D4D49),
    incoming = Color(0xFF1E2A2E),
    wallpaper = Color(0xFF0E1517),
    tick = Color(0xFF8FA3A0),
    tickRead = Color(0xFF5EB4FF),
    meta = Color(0xFF93A6A3),
)

val LocalChatColors = staticCompositionLocalOf { LightChat }

private val Light = lightColorScheme(
    primary = Color(0xFF0B5E5A),
    onPrimary = Color.White,
    primaryContainer = Color(0xFFB7EBE2),
    onPrimaryContainer = Color(0xFF00201D),
    secondary = Color(0xFF1F9E7A),
    onSecondary = Color.White,
    background = Color(0xFFF7FAF8),
    surface = Color(0xFFFFFFFF),
    surfaceVariant = Color(0xFFE2EAE7),
    onSurfaceVariant = Color(0xFF55625F),
)

private val Dark = darkColorScheme(
    primary = Color(0xFF7FD8C9),
    onPrimary = Color(0xFF00332F),
    primaryContainer = Color(0xFF0F4E49),
    onPrimaryContainer = Color(0xFFB7EBE2),
    secondary = Color(0xFF5FD4AE),
    onSecondary = Color(0xFF003828),
    background = Color(0xFF0E1517),
    surface = Color(0xFF141D20),
    surfaceVariant = Color(0xFF253235),
    onSurfaceVariant = Color(0xFFB4C4C0),
)

@Composable
fun NmTheme(content: @Composable () -> Unit) {
    val dark = isSystemInDarkTheme()
    androidx.compose.runtime.CompositionLocalProvider(LocalChatColors provides if (dark) DarkChat else LightChat) {
        MaterialTheme(colorScheme = if (dark) Dark else Light, content = content)
    }
}
