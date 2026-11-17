package app.novomessenger.ui

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import uniffi.nm_ffi.MessageStatus
import uniffi.nm_ffi.qrImage
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Date
import java.util.Locale

private val avatarColors = listOf(
    Color(0xFF0B7A6F), Color(0xFF8E5BD8), Color(0xFFD9822B), Color(0xFF2E7BD6),
    Color(0xFFC2416B), Color(0xFF4C8C2B), Color(0xFF9C6B30), Color(0xFF1F8FA8),
)

/** Círculo com a inicial do nome, cor fixa por pessoa. */
@Composable
fun Avatar(name: String, key: String, size: Dp = 48.dp) {
    val color = avatarColors[(key.hashCode() and 0x7fffffff) % avatarColors.size]
    Box(
        Modifier.size(size).clip(CircleShape).background(color),
        contentAlignment = Alignment.Center,
    ) {
        Text(
            name.trim().firstOrNull()?.uppercase() ?: "?",
            color = Color.White,
            fontWeight = FontWeight.SemiBold,
            fontSize = (size.value * 0.42f).sp,
        )
    }
}

/** ◷ salva · ✓ enviada · ✓✓ entregue · ✓✓ azul lida */
@Composable
fun Ticks(status: MessageStatus) {
    val c = LocalChatColors.current
    val (glyph, color, label) = when (status) {
        MessageStatus.PENDING -> Triple("◷", c.tick, "aguardando aparelho por perto")
        MessageStatus.SENT -> Triple("✓", c.tick, "enviada")
        MessageStatus.DELIVERED -> Triple("✓✓", c.tick, "entregue")
        MessageStatus.READ -> Triple("✓✓", c.tickRead, "lida")
    }
    Text(
        glyph,
        color = color,
        fontSize = 13.sp,
        fontWeight = FontWeight.Bold,
        modifier = Modifier.padding(start = 4.dp).semantics { contentDescription = label },
    )
}

/** Hora se for hoje, "ontem", ou a data. */
fun shortTime(ms: ULong): String {
    val d = Date(ms.toLong())
    val now = Calendar.getInstance()
    val then = Calendar.getInstance().apply { time = d }
    val sameDay = now.get(Calendar.YEAR) == then.get(Calendar.YEAR) && now.get(Calendar.DAY_OF_YEAR) == then.get(Calendar.DAY_OF_YEAR)
    if (sameDay) return SimpleDateFormat("HH:mm", Locale("pt", "BR")).format(d)
    now.add(Calendar.DAY_OF_YEAR, -1)
    val yesterday = now.get(Calendar.YEAR) == then.get(Calendar.YEAR) && now.get(Calendar.DAY_OF_YEAR) == then.get(Calendar.DAY_OF_YEAR)
    if (yesterday) return "ontem"
    return SimpleDateFormat("dd/MM/yy", Locale("pt", "BR")).format(d)
}

fun clockTime(ms: ULong): String = SimpleDateFormat("HH:mm", Locale("pt", "BR")).format(Date(ms.toLong()))

/** QR Code desenhado a partir do gerador do núcleo Rust. */
@Composable
fun QrCode(text: String, modifier: Modifier = Modifier) {
    val qr = remember(text) { qrImage(text) } ?: return
    val n = qr.size.toInt()
    Canvas(
        modifier
            .clip(RoundedCornerShape(16.dp))
            .background(Color.White)
            .padding(14.dp)
            .semantics { contentDescription = "QR Code do seu contato" },
    ) {
        val cell = size.minDimension / n
        for (y in 0 until n) {
            for (x in 0 until n) {
                if (qr.modules[y * n + x].toInt() != 0) {
                    drawRect(Color.Black, topLeft = Offset(x * cell, y * cell), size = Size(cell + 0.6f, cell + 0.6f))
                }
            }
        }
    }
}

@Composable
fun SectionLabel(text: String, modifier: Modifier = Modifier) {
    Text(
        text.uppercase(),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.primary,
        letterSpacing = 0.8.sp,
        modifier = modifier,
    )
}
