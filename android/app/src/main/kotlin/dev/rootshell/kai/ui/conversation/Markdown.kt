package dev.rootshell.kai.ui.conversation

import android.util.Log
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.platform.LocalUriHandler
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.LinkInteractionListener
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.core.ffi.MdBlock
import dev.rootshell.kai.core.ffi.MdBlockKind
import dev.rootshell.kai.core.ffi.MdSpan

/**
 * A message's Markdown as the core parsed it (design M14): flat blocks of styled spans, rendered
 * with composables of our own in Material's typography. Links open in the browser (or the mail app)
 * on a tap: the core keeps only `http`, `https` and `mailto` targets of the model's output, and a
 * phone with no app for one says so in the log instead of throwing in the process that holds the
 * feed. A table shows as its source text (Later).
 */
@Composable
fun Markdown(blocks: List<MdBlock>, modifier: Modifier = Modifier, colour: Color = Color.Unspecified) {
    Column(modifier) {
        blocks.forEach { block -> Block(block, colour) }
    }
}

@Composable
private fun Block(block: MdBlock, colour: Color) {
    val indent = (block.depth.toInt() * 12 + block.quote.toInt() * 12).dp
    val body = MaterialTheme.typography.bodyLarge
    val quoted = block.quote > 0u
    val textColour = if (quoted) MaterialTheme.colorScheme.onSurfaceVariant else colour
    val padding = Modifier.padding(start = indent, top = 4.dp, bottom = 4.dp)
    when (val kind = block.kind) {
        MdBlockKind.Paragraph -> Text(spans(block.spans), padding, style = body, color = textColour)
        is MdBlockKind.Heading -> Text(
            spans(block.spans),
            padding,
            style = when (kind.level.toInt()) {
                1 -> MaterialTheme.typography.headlineSmall
                2 -> MaterialTheme.typography.titleLarge
                else -> MaterialTheme.typography.titleMedium
            },
            color = textColour,
        )
        is MdBlockKind.ListItem -> Row(padding) {
            Text(kind.marker, style = body, color = textColour, modifier = Modifier.padding(end = 8.dp))
            Text(spans(block.spans), style = body, color = textColour)
        }
        is MdBlockKind.CodeBlock -> Code(block.text, padding)
        MdBlockKind.Table -> Code(block.text, padding)
        MdBlockKind.Rule -> HorizontalDivider(padding)
    }
}

@Composable
private fun Code(text: String, modifier: Modifier) {
    Surface(color = MaterialTheme.colorScheme.surfaceVariant, shape = MaterialTheme.shapes.small, modifier = modifier.fillMaxWidth()) {
        Text(
            text,
            style = TextStyle(fontFamily = FontFamily.Monospace, fontSize = MaterialTheme.typography.bodyMedium.fontSize),
            modifier = Modifier.horizontalScroll(rememberScrollState()).padding(8.dp),
        )
    }
}

@Composable
private fun spans(spans: List<MdSpan>): AnnotatedString {
    val link = TextLinkStyles(SpanStyle(color = MaterialTheme.colorScheme.primary, textDecoration = TextDecoration.Underline))
    val uriHandler = LocalUriHandler.current
    val open = LinkInteractionListener { annotation ->
        val url = (annotation as? LinkAnnotation.Url)?.url ?: return@LinkInteractionListener
        runCatching { uriHandler.openUri(url) }
            // Not its message: it quotes the link, which is the thread's text.
            .onFailure { Log.w("kai", "a link did not open: ${it.javaClass.simpleName}") }
    }
    val codeBackground = MaterialTheme.colorScheme.surfaceVariant
    return buildAnnotatedString {
        spans.forEach { s ->
            val style = SpanStyle(
                fontWeight = if (s.strong) FontWeight.Bold else null,
                fontStyle = if (s.emphasis) FontStyle.Italic else null,
                textDecoration = if (s.strike) TextDecoration.LineThrough else null,
                fontFamily = if (s.code) FontFamily.Monospace else null,
                background = if (s.code) codeBackground else Color.Unspecified,
            )
            val target = s.link
            if (target != null) {
                withLink(LinkAnnotation.Url(target, link, open)) { withStyle(style) { append(s.text) } }
            } else {
                withStyle(style) { append(s.text) }
            }
        }
    }
}
