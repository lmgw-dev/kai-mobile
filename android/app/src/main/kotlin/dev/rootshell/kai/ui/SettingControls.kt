package dev.rootshell.kai.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.selection.toggleable
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.clickable
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.KeyboardArrowDown
import androidx.compose.material.icons.filled.KeyboardArrowUp
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import dev.rootshell.kai.R
import dev.rootshell.kai.ui.screens.WholeNumber

/*
 * The settings pages' controls (design §3.4, 2026-10-19): a setting is a human label with a plain
 * supporting line, never its key. Sections have headers; a number has its unit as a suffix.
 */

/** A section's header. */
@Composable
fun SectionHeader(text: String, modifier: Modifier = Modifier) {
    Text(
        text,
        style = MaterialTheme.typography.titleSmall,
        color = MaterialTheme.colorScheme.primary,
        modifier = modifier.padding(top = 16.dp, bottom = 4.dp),
    )
}

/** A line of explanation under a header or a control. */
@Composable
fun SupportingText(text: String, modifier: Modifier = Modifier) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = modifier)
}

/** A whole-number field: label, unit as the suffix, the numeric keyboard; an optional one may be empty. */
@Composable
fun SettingNumber(
    label: Int,
    help: Int,
    unit: Int,
    value: String,
    optional: Boolean = false,
    placeholder: Int? = null,
    onChange: (String) -> Unit,
) {
    val valid = WholeNumber.valid(value, optional)
    OutlinedTextField(
        value = value,
        onValueChange = onChange,
        label = { Text(stringResource(label)) },
        placeholder = placeholder?.let { { Text(stringResource(it)) } },
        suffix = { Text(stringResource(unit)) },
        supportingText = { Text(stringResource(if (valid) help else R.string.set_whole_number_hint)) },
        isError = !valid,
        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
}

/** A choice among several, one row each with its plain-language line; the whole row selects. */
@Composable
fun ChoiceRow(label: String, selected: Boolean, help: String? = null, onSelect: () -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).selectable(selected = selected, role = Role.RadioButton, onClick = onSelect),
        verticalAlignment = Alignment.Top,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        RadioButton(selected = selected, onClick = null, modifier = Modifier.padding(top = 12.dp, end = 8.dp))
        Column(Modifier.weight(1f).padding(vertical = 12.dp)) {
            Text(label, style = MaterialTheme.typography.bodyLarge)
            if (help != null) SupportingText(help)
        }
    }
}

/** A switch with its label and a plain-language line; the whole row toggles it. */
@Composable
fun SettingSwitch(label: String, help: String, on: Boolean, onChange: (Boolean) -> Unit) {
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).toggleable(value = on, role = Role.Switch, onValueChange = onChange),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Column(Modifier.weight(1f).padding(vertical = 8.dp)) {
            Text(label, style = MaterialTheme.typography.bodyLarge)
            SupportingText(help)
        }
        Switch(checked = on, onCheckedChange = null)
    }
}

/** A collapsed group of rarely changed values; its header says whether it is open to TalkBack. */
@Composable
fun AdvancedGroup(content: @Composable () -> Unit) {
    var open by rememberSaveable { mutableStateOf(false) }
    val state = stringResource(if (open) R.string.set_advanced_open else R.string.set_advanced_closed)
    Row(
        Modifier.fillMaxWidth().heightIn(min = 48.dp).clickable(role = Role.Button) { open = !open }.semantics { stateDescription = state },
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            stringResource(R.string.set_advanced),
            style = MaterialTheme.typography.titleSmall,
            color = MaterialTheme.colorScheme.primary,
            modifier = Modifier.weight(1f),
        )
        Icon(if (open) Icons.Filled.KeyboardArrowUp else Icons.Filled.KeyboardArrowDown, contentDescription = null)
    }
    if (open) Column(verticalArrangement = Arrangement.spacedBy(8.dp)) { content() }
}
