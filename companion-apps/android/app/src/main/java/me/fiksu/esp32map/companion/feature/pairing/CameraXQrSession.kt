package me.fiksu.esp32map.companion.feature.pairing

import android.content.Context
import androidx.annotation.OptIn
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.ImageProxy
import androidx.camera.core.ExperimentalGetImage
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleOwner
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import java.util.concurrent.Executor
import java.util.concurrent.Executors

/**
 * CameraX + ML Kit QR session used by the pairing screen in production.
 * Tests replace this with a [QrCaptureSession] fake.
 *
 * Wiring lives in `PairingFlowScreen.kt`. The session is bound to the
 * Compose host's lifecycle owner so the camera stops cleanly on
 * navigation away.
 */
class CameraXQrSession(
    private val context: Context,
    private val lifecycleOwner: LifecycleOwner,
    private val previewView: PreviewView,
) : QrCaptureSession {
    private var cameraProvider: ProcessCameraProvider? = null
    private val analysisExecutor: Executor = Executors.newSingleThreadExecutor()
    private val scanner = BarcodeScanning.getClient(
        BarcodeScannerOptions.Builder()
            .setBarcodeFormats(Barcode.FORMAT_QR_CODE)
            .build(),
    )

    override fun start(onScan: (String?) -> Unit) {
        val futureProvider = ProcessCameraProvider.getInstance(context)
        futureProvider.addListener({
            val provider = futureProvider.get()
            cameraProvider = provider
            val preview = Preview.Builder().build().apply {
                setSurfaceProvider(previewView.surfaceProvider)
            }
            val analysis = ImageAnalysis.Builder()
                .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
                .build()
            analysis.setAnalyzer(analysisExecutor) { proxy ->
                analyzeFrame(proxy, onScan)
            }
            provider.unbindAll()
            provider.bindToLifecycle(
                lifecycleOwner,
                androidx.camera.core.CameraSelector.DEFAULT_BACK_CAMERA,
                preview,
                analysis,
            )
        }, ContextCompat.getMainExecutor(context))
    }

    override fun stop() {
        cameraProvider?.unbindAll()
        cameraProvider = null
        scanner.close()
    }

    @OptIn(ExperimentalGetImage::class)
    private fun analyzeFrame(proxy: ImageProxy, onScan: (String?) -> Unit) {
        val media = proxy.image
        if (media == null) {
            proxy.close()
            return
        }
        val input = InputImage.fromMediaImage(media, proxy.imageInfo.rotationDegrees)
        scanner.process(input)
            .addOnSuccessListener { barcodes ->
                if (barcodes.isNotEmpty()) {
                    onScan(barcodes.first().rawValue)
                }
            }
            .addOnCompleteListener { proxy.close() }
    }
}
