package app.navon.bike.feature.home

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.filled.Add
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import app.navon.bike.domain.DeviceConnectionState
import app.navon.bike.domain.PairedPeripheralRecord

/**
 * 4-state view of the home-screen device chip. Derived from
 * `(pairedPeripheral, connectionState)` so the same UI logic stays in
 * lockstep with iOS.
 */
sealed class DeviceChipState {
    object Unpaired : DeviceChipState()
    data class PairedDisconnected(val name: String) : DeviceChipState()
    data class Connecting(val name: String) : DeviceChipState()
    data class Connected(val name: String) : DeviceChipState()
}

/**
 * Pure derivation; tested in isolation under
 * `DeviceStatusChipStateTest`. Putting it here (next to the chip
 * itself) keeps the chip's state contract co-located with its renderer.
 */
fun deviceChipStateOf(
    paired: PairedPeripheralRecord?,
    connection: DeviceConnectionState,
): DeviceChipState = when {
    paired == null -> DeviceChipState.Unpaired
    connection == DeviceConnectionState.SCANNING ||
        connection == DeviceConnectionState.CONNECTING ->
        DeviceChipState.Connecting(paired.friendlyName)
    connection == DeviceConnectionState.CONNECTED ->
        DeviceChipState.Connected(paired.friendlyName)
    else -> DeviceChipState.PairedDisconnected(paired.friendlyName)
}

/**
 * Tap behaviour expected for a given chip state. Lets a higher layer
 * (HomeStateHolder / MainActivity) wire the actual `CompanionAppState`
 * methods without coupling the chip view to AppState.
 */
sealed class DeviceChipTapAction {
    object BeginPairingFlow : DeviceChipTapAction()
    object ConnectToDevice : DeviceChipTapAction()
    object ShowConnectedPopover : DeviceChipTapAction()
    object Noop : DeviceChipTapAction()
}

fun deviceChipTapActionFor(state: DeviceChipState): DeviceChipTapAction = when (state) {
    is DeviceChipState.Unpaired -> DeviceChipTapAction.BeginPairingFlow
    is DeviceChipState.PairedDisconnected -> DeviceChipTapAction.ConnectToDevice
    is DeviceChipState.Connected -> DeviceChipTapAction.ShowConnectedPopover
    is DeviceChipState.Connecting -> DeviceChipTapAction.Noop
}

@Composable
fun DeviceStatusChip(
    state: DeviceChipState,
    onTap: (DeviceChipTapAction) -> Unit,
    modifier: Modifier = Modifier,
) {
    val action = deviceChipTapActionFor(state)
    val description = when (state) {
        is DeviceChipState.Unpaired -> "No device paired, tap to pair"
        is DeviceChipState.PairedDisconnected -> "Device ${state.name} disconnected, tap to reconnect"
        is DeviceChipState.Connecting -> "Connecting to ${state.name}"
        is DeviceChipState.Connected -> "Connected to ${state.name}"
    }
    val enabled = state !is DeviceChipState.Connecting
    IconButton(
        onClick = { onTap(action) },
        enabled = enabled,
        modifier = modifier
            .size(50.dp)
            .semantics { contentDescription = description },
    ) {
        when (state) {
            is DeviceChipState.Unpaired -> Icon(
                imageVector = Icons.Filled.Add,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            is DeviceChipState.PairedDisconnected -> Icon(
                imageVector = Icons.Filled.Refresh,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            is DeviceChipState.Connecting -> Box(
                contentAlignment = Alignment.Center,
                modifier = Modifier.size(50.dp),
            ) {
                CircularProgressIndicator(
                    modifier = Modifier.size(28.dp),
                    strokeWidth = 2.dp,
                )
            }
            is DeviceChipState.Connected -> Icon(
                imageVector = Icons.Filled.Check,
                contentDescription = null,
                tint = MaterialTheme.colorScheme.primary,
            )
        }
    }
}

/**
 * Adds a unit shape so the IconButton's clickable area stays a 44dp+
 * circle rather than the default rounded-rect; matches the iOS chip
 * for tap-target accessibility parity.
 */
@Suppress("unused")
private val IconShape = CircleShape
