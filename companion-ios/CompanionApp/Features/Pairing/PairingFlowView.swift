import SwiftUI
import UIKit

/// Pairing flow sheet. Three steps: instructions, camera capture, then a
/// progress/result panel while the BLE confirm flow runs.
struct PairingFlowView: View {
    @EnvironmentObject private var appModel: AppModel
    @StateObject private var viewModel: PairingFlowViewModel
    @StateObject private var cameraSession = StoredQrCaptureSession()

    init(viewModel: PairingFlowViewModel? = nil, appModel: AppModel? = nil) {
        if let viewModel {
            _viewModel = StateObject(wrappedValue: viewModel)
        } else {
            // Real session is wired lazily on first appearance so the camera
            // pipeline isn't spun up until the sheet opens.
            let bootstrap = PairingFlowViewModelBootstrap()
            _viewModel = StateObject(wrappedValue: bootstrap.makeViewModel(appModel: appModel))
        }
    }

    var body: some View {
        VStack(spacing: 16) {
            switch viewModel.pairingState {
            case .instructions:
                instructionsStep
            case .scanning:
                cameraStep
            case .connecting, .succeeded, .failed:
                progressStep
            }
        }
        .padding()
        .interactiveDismissDisabled(true)
        .onAppear {
            pairingLog.notice("PairingFlowView.onAppear — sheet rendered, step=\(String(describing: self.viewModel.pairingState), privacy: .public)")
        }
    }

    private var instructionsStep: some View {
        VStack(spacing: 16) {
            Text("Pair a device")
                .font(.title2.weight(.semibold))
            Text("Hold your phone so the device's screen is in frame, then tap Start when you see the QR code.")
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
            Button("Start") {
                Task { await viewModel.enterScanningStep() }
            }
            .buttonStyle(.borderedProminent)
            Button("Cancel", role: .cancel) {
                viewModel.cancel()
            }
        }
    }

    private var cameraStep: some View {
        VStack(spacing: 16) {
            switch viewModel.permissionDescriptor {
            case .denied(.openSettings):
                deniedCard
            case .notDetermined, .authorized:
                ZStack {
                    Color.black.ignoresSafeArea()
                    Text("Align the QR code in the frame")
                        .foregroundStyle(.white)
                        .padding()
                }
                .frame(height: 320)
                .clipShape(RoundedRectangle(cornerRadius: 18))
            }
            if let message = viewModel.scanErrorMessage {
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(.red)
            }
            if viewModel.scanGuidance == .centerOnQr {
                Text("Center the QR code in the frame and hold steady.")
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Button("Cancel", role: .cancel) {
                viewModel.cancel()
            }
        }
    }

    private var progressStep: some View {
        VStack(spacing: 16) {
            switch viewModel.pairingState {
            case .connecting:
                ProgressView("Connecting…")
            case .succeeded:
                Label("Paired", systemImage: "checkmark.seal.fill")
                    .font(.title3.weight(.semibold))
            case .failed(let message):
                Label("Pairing failed", systemImage: "xmark.octagon.fill")
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(.red)
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            default:
                EmptyView()
            }
            Button("Cancel", role: .cancel) {
                viewModel.cancel()
            }
        }
    }

    private var deniedCard: some View {
        VStack(spacing: 12) {
            Image(systemName: "camera.fill")
                .font(.largeTitle)
            Text("Camera access is required to scan the pairing code.")
                .multilineTextAlignment(.center)
            Button("Open Settings") {
                if let url = URL(string: UIApplication.openSettingsURLString) {
                    UIApplication.shared.open(url)
                }
            }
            .buttonStyle(.borderedProminent)
        }
        .padding()
    }
}

/// Holder for the lazy AVFoundation session so SwiftUI's `@StateObject` can
/// keep a reference across re-renders without re-creating the camera pipeline.
@MainActor
private final class StoredQrCaptureSession: ObservableObject {
    let session = AVFoundationQrCaptureSession()
}

@MainActor
private struct PairingFlowViewModelBootstrap {
    func makeViewModel(appModel: AppModel?) -> PairingFlowViewModel {
        PairingFlowViewModel(
            session: AVFoundationQrCaptureSession(),
            permission: AVFoundationCameraPermissionProvider(),
            appModel: appModel
        )
    }
}
