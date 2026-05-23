import SwiftUI
import UIKit

/// Pairing flow sheet. Three steps: instructions, camera capture, then a
/// progress/result panel while the BLE confirm flow runs.
struct PairingFlowView: View {
    @EnvironmentObject private var appModel: AppModel
    @StateObject private var controller: PairingFlowController

    init(appModel: AppModel? = nil) {
        // Capture the appModel ref outside the closure: SwiftUI calls the
        // wrappedValue closure once on first render, but we still want a
        // single AVFoundationQrCaptureSession shared by both the VM (for
        // metadata callbacks) and the preview layer (for frames on screen).
        let captured = appModel
        _controller = StateObject(wrappedValue: PairingFlowController(appModel: captured))
    }

    var body: some View {
        VStack(spacing: 16) {
            switch controller.viewModel.pairingState {
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
            pairingLog.notice("PairingFlowView.onAppear — sheet rendered, step=\(String(describing: self.controller.viewModel.pairingState), privacy: .public)")
        }
        .onDisappear {
            controller.session.tearDown()
        }
    }

    private var viewModel: PairingFlowViewModel { controller.viewModel }

    private var instructionsStep: some View {
        VStack(spacing: 16) {
            Text(T.string("pairing.title"))
                .font(.title2.weight(.semibold))
            Text(T.string("pairing.intro"))
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
            Button(T.string("pairing.start")) {
                Task { await viewModel.enterScanningStep() }
            }
            .buttonStyle(.borderedProminent)
            Button(T.string("pairing.cancel"), role: .cancel) {
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
                cameraPreviewCard
            }
            if let message = viewModel.scanErrorMessage {
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(.red)
            }
            if viewModel.scanGuidance == .centerOnQr {
                Text(T.string("pairing.scanInstruction"))
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            }
            Button(T.string("pairing.cancel"), role: .cancel) {
                viewModel.cancel()
            }
        }
    }

    /// Live AVFoundation preview. The session is owned by `controller` and
    /// shared by the QR-metadata pipeline that drives the VM.
    private var cameraPreviewCard: some View {
        ZStack {
            PairingCameraPreview(session: controller.session)
                .ignoresSafeArea(edges: .horizontal)
            // Crosshair overlay. The plan asks for a frame around the
            // center 70% of the preview; a stroked rounded rect gives the
            // user a clear target without blocking the AVFoundation
            // metadata detector (which scans the whole frame regardless).
            GeometryReader { proxy in
                let dim = min(proxy.size.width, proxy.size.height) * 0.7
                Path { path in
                    let rect = CGRect(
                        x: (proxy.size.width - dim) / 2,
                        y: (proxy.size.height - dim) / 2,
                        width: dim,
                        height: dim
                    )
                    path.addRoundedRect(in: rect, cornerSize: CGSize(width: 12, height: 12))
                }
                .stroke(Color.white.opacity(0.85), lineWidth: 2)
            }
            VStack {
                Spacer()
                Text(T.string("pairing.alignQr"))
                    .font(.footnote.weight(.semibold))
                    .foregroundStyle(.white)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .background(.black.opacity(0.55), in: Capsule())
                    .padding(.bottom, 16)
            }
        }
        .frame(height: 380)
        .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
        .onAppear {
            // Camera permission may be pending here on first run; start
            // the session unconditionally — AVFoundation simply produces
            // no frames if permission is denied (we surface the denial
            // through `permissionDescriptor` in the next layout pass).
            controller.session.start()
        }
    }

    private var progressStep: some View {
        VStack(spacing: 16) {
            switch viewModel.pairingState {
            case .connecting:
                ProgressView("Connecting…")
            case .succeeded:
                Label(T.string("pairing.paired"), systemImage: "checkmark.seal.fill")
                    .font(.title3.weight(.semibold))
            case .failed(let message):
                Label(T.string("pairing.failed"), systemImage: "xmark.octagon.fill")
                    .font(.title3.weight(.semibold))
                    .foregroundStyle(.red)
                Text(message)
                    .font(.footnote)
                    .foregroundStyle(.secondary)
            default:
                EmptyView()
            }
            Button(T.string("pairing.cancel"), role: .cancel) {
                viewModel.cancel()
            }
        }
    }

    private var deniedCard: some View {
        VStack(spacing: 12) {
            Image(systemName: "camera.fill")
                .font(.largeTitle)
            Text(T.string("pairing.cameraDenied"))
                .multilineTextAlignment(.center)
            Button(T.string("pairing.openSettings")) {
                if let url = URL(string: UIApplication.openSettingsURLString) {
                    UIApplication.shared.open(url)
                }
            }
            .buttonStyle(.borderedProminent)
        }
        .padding()
    }
}

/// Owns the AVFoundation session + VM together so SwiftUI's `@StateObject`
/// keeps them paired across re-renders without re-creating either. The VM's
/// `session` reference and the preview layer's `session` reference are the
/// same `AVFoundationQrCaptureSession` instance — this is what fixes the
/// "black rectangle" bug where two unrelated sessions ended up live.
@MainActor
final class PairingFlowController: ObservableObject {
    let session: AVFoundationQrCaptureSession
    let viewModel: PairingFlowViewModel

    init(appModel: AppModel?) {
        let session = AVFoundationQrCaptureSession()
        self.session = session
        self.viewModel = PairingFlowViewModel(
            session: session,
            permission: AVFoundationCameraPermissionProvider(),
            appModel: appModel
        )
    }
}
