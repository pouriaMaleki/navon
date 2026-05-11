package app.navon.bike.feature.pairing

import android.Manifest
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.provider.Settings
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.view.PreviewView
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalLifecycleOwner
import androidx.lifecycle.findViewTreeLifecycleOwner
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import kotlinx.coroutines.launch
import app.navon.bike.app.CompanionAppState
import app.navon.bike.domain.PairingFlowState

/**
 * Three-step pairing flow: instructions → camera/scan → confirm. Cancel
 * is available at every step. Mirrors the iOS-side `PairingFlowView`.
 *
 * The screen owns a [PairingFlowViewModel] for the QR-scan step's state
 * machine. Tests cover the view model directly via
 * `PairingFlowViewModelTest`; the camera + Compose plumbing here is
 * exercised on-device.
 */
@Composable
fun PairingFlowScreen(
    appState: CompanionAppState,
    onClose: () -> Unit,
    captureSessionFactory: (PreviewView) -> QrCaptureSession = { previewView ->
        CameraXQrSession(
            context = previewView.context,
            // Compose hosts always have a lifecycle owner; this cast is
            // safe in practice but use the local value where available.
            lifecycleOwner = previewView.findViewTreeLifecycleOwner()
                ?: error("PairingFlowScreen requires a Compose LifecycleOwner"),
            previewView = previewView,
        )
    },
) {
    val scope = rememberCoroutineScope()
    val viewModel = remember {
        PairingFlowViewModel(onPayloadDecoded = { payload ->
            scope.launch {
                appState.completePairing(payload)
                appState.pairingState.let(::syncBack).also { /* no-op hook */ }
            }
        })
    }

    when (val pairingState = appState.pairingState) {
        is PairingFlowState.Idle, is PairingFlowState.Instructions -> InstructionsStep(
            onContinue = {
                // Flip the device's panel to QR before opening the
                // camera. `prepareDeviceForPairing` connects + writes
                // the unencrypted `pairing_request` characteristic, so
                // by the time the camera comes up the QR is already
                // showing on the device.
                scope.launch {
                    runCatching { appState.prepareDeviceForPairing() }
                }
            },
            onCancel = onClose,
        )
        is PairingFlowState.Scanning -> ScanStep(
            viewModel = viewModel,
            captureSessionFactory = captureSessionFactory,
            onCancel = {
                viewModel.cancel()
                onClose()
            },
        )
        is PairingFlowState.Connecting,
        is PairingFlowState.Confirming -> ProgressStep(
            label = if (pairingState is PairingFlowState.Confirming) {
                "Confirming pairing…"
            } else {
                "Connecting to device…"
            },
        )
        is PairingFlowState.Succeeded -> SucceededStep(onClose = onClose)
        is PairingFlowState.Failed -> FailedStep(
            reason = pairingState.reason,
            onRetry = { appState.beginPairingFlow() },
            onCancel = onClose,
        )
    }
}

private fun syncBack(state: PairingFlowState): PairingFlowState = state

@Composable
private fun InstructionsStep(onContinue: () -> Unit, onCancel: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(
            text = "Pair with your device",
            style = MaterialTheme.typography.headlineSmall,
        )
        Text(
            text = "Power on your ESP32 bike minimap. The screen will show a QR code. " +
                "Hold your phone steady about 30 cm away and we'll scan it for you.",
            style = MaterialTheme.typography.bodyMedium,
        )
        Spacer(Modifier.height(24.dp))
        Button(onClick = onContinue, modifier = Modifier.fillMaxWidth()) {
            Text("Open camera")
        }
        OutlinedButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) {
            Text("Cancel")
        }
    }
}

@Composable
private fun ScanStep(
    viewModel: PairingFlowViewModel,
    captureSessionFactory: (PreviewView) -> QrCaptureSession,
    onCancel: () -> Unit,
) {
    val context = LocalContext.current
    val lifecycleOwner = LocalLifecycleOwner.current
    var permissionGranted by remember {
        mutableStateOf(
            ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) ==
                PackageManager.PERMISSION_GRANTED,
        )
    }
    val permissionLauncher = rememberLauncherForActivityResult(
        ActivityResultContracts.RequestPermission(),
    ) { granted -> permissionGranted = granted }

    LaunchedEffect(Unit) {
        if (!permissionGranted) {
            permissionLauncher.launch(Manifest.permission.CAMERA)
        }
    }

    if (!permissionGranted) {
        PermissionDeniedStep(
            onOpenSettings = {
                val intent = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS).apply {
                    data = Uri.fromParts("package", context.packageName, null)
                }
                context.startActivity(intent)
            },
            onCancel = onCancel,
        )
        return
    }

    Box(modifier = Modifier.fillMaxSize().background(Color.Black)) {
        AndroidView(
            modifier = Modifier.fillMaxSize(),
            factory = { ctx ->
                PreviewView(ctx).also { preview ->
                    val session = captureSessionFactory(preview)
                    session.start { rawValue -> viewModel.handleScannedRawValue(rawValue) }
                }
            },
        )
        Column(
            modifier = Modifier
                .fillMaxSize()
                .padding(24.dp),
            verticalArrangement = Arrangement.SpaceBetween,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Text(
                text = "Point at the QR code on your device",
                color = Color.White,
                style = MaterialTheme.typography.bodyLarge,
            )
            viewModel.scanErrorMessage?.let { message ->
                Text(
                    text = message,
                    color = Color.White,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            if (viewModel.scanGuidance == ScanGuidance.CenterOnQr) {
                Text(
                    text = "Center the code in the frame",
                    color = Color.White,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            OutlinedButton(onClick = onCancel) {
                Text("Cancel")
            }
        }
    }
}

@Composable
private fun ProgressStep(label: String) {
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            CircularProgressIndicator()
            Spacer(Modifier.height(16.dp))
            Text(text = label)
        }
    }
}

@Composable
private fun SucceededStep(onClose: () -> Unit) {
    Box(modifier = Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
        Column(horizontalAlignment = Alignment.CenterHorizontally) {
            Text(text = "Paired!", style = MaterialTheme.typography.headlineSmall)
            Spacer(Modifier.height(8.dp))
            Text(text = "You're connected. Heading back to home…")
            Spacer(Modifier.height(16.dp))
            TextButton(onClick = onClose) { Text("Close") }
        }
    }
}

@Composable
private fun FailedStep(reason: String, onRetry: () -> Unit, onCancel: () -> Unit) {
    AlertDialog(
        onDismissRequest = onCancel,
        title = { Text("Pairing failed") },
        text = { Text(reason) },
        confirmButton = {
            TextButton(onClick = onRetry) { Text("Try again") }
        },
        dismissButton = {
            TextButton(onClick = onCancel) { Text("Cancel") }
        },
    )
}

@Composable
private fun PermissionDeniedStep(onOpenSettings: () -> Unit, onCancel: () -> Unit) {
    Column(
        modifier = Modifier
            .fillMaxSize()
            .padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text(
            text = "Camera permission needed",
            style = MaterialTheme.typography.headlineSmall,
        )
        Text(
            text = "We need camera access to scan the pairing QR. Open Settings to grant it.",
            style = MaterialTheme.typography.bodyMedium,
        )
        Button(onClick = onOpenSettings, modifier = Modifier.fillMaxWidth()) {
            Text("Open Settings")
        }
        OutlinedButton(onClick = onCancel, modifier = Modifier.fillMaxWidth()) {
            Text("Cancel")
        }
    }
}

