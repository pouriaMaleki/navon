import AVFoundation
import SwiftUI
import UIKit

/// AVFoundation-backed `QrCaptureSession`. Wraps a single-input
/// `AVCaptureSession` whose `.qr` metadata feeds the `onScan` callback.
/// Production code path; `FakeQrCaptureSession` covers the same protocol in
/// unit tests.
@MainActor
final class AVFoundationQrCaptureSession: NSObject, ObservableObject, QrCaptureSession, AVCaptureMetadataOutputObjectsDelegate {
    var onScan: ((String) -> Void)?

    let session = AVCaptureSession()
    private let metadataOutput = AVCaptureMetadataOutput()
    private(set) var configureDidSucceed = false

    override init() {
        super.init()
        configure()
    }

    private func configure() {
        guard let device = AVCaptureDevice.default(.builtInWideAngleCamera, for: .video, position: .back),
              let input = try? AVCaptureDeviceInput(device: device),
              session.canAddInput(input),
              session.canAddOutput(metadataOutput)
        else {
            pairingLog.error("AVFoundationQrCaptureSession.configure failed — no camera available or input/output rejected")
            return
        }
        session.addInput(input)
        session.addOutput(metadataOutput)
        metadataOutput.setMetadataObjectsDelegate(self, queue: .main)
        metadataOutput.metadataObjectTypes = [.qr]
        configureDidSucceed = true
        pairingLog.notice("AVFoundationQrCaptureSession configured (preset=\(self.session.sessionPreset.rawValue, privacy: .public))")
    }

    func start() {
        guard configureDidSucceed else {
            pairingLog.error("AVFoundationQrCaptureSession.start skipped — configure failed earlier")
            return
        }
        guard !session.isRunning else { return }
        // `startRunning` blocks; do it off the main thread per Apple's docs
        // and let CoreAnimation/CoreVideo pull frames into the preview layer
        // on its own queue.
        Task.detached { [session] in
            session.startRunning()
            await MainActor.run {
                pairingLog.notice("AVFoundationQrCaptureSession.start — running=\(session.isRunning, privacy: .public)")
            }
        }
    }

    func tearDown() {
        if session.isRunning {
            session.stopRunning()
        }
        onScan = nil
        pairingLog.notice("AVFoundationQrCaptureSession.tearDown — running=\(self.session.isRunning, privacy: .public)")
    }

    nonisolated func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput metadataObjects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        for object in metadataObjects {
            guard let machineReadable = object as? AVMetadataMachineReadableCodeObject,
                  machineReadable.type == .qr,
                  let raw = machineReadable.stringValue
            else { continue }
            DispatchQueue.main.async { [weak self] in
                self?.onScan?(raw)
            }
        }
    }
}

/// SwiftUI bridge that hosts the AVFoundation preview layer.
struct PairingCameraPreview: UIViewRepresentable {
    let session: AVFoundationQrCaptureSession

    func makeUIView(context: Context) -> PreviewView {
        let view = PreviewView()
        view.previewLayer.session = session.session
        view.previewLayer.videoGravity = .resizeAspectFill
        return view
    }

    func updateUIView(_ uiView: PreviewView, context: Context) {}

    final class PreviewView: UIView {
        override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }
        var previewLayer: AVCaptureVideoPreviewLayer { layer as! AVCaptureVideoPreviewLayer }
    }
}

/// Production permission provider backed by `AVCaptureDevice`.
struct AVFoundationCameraPermissionProvider: CameraPermissionProvider {
    func currentStatus() -> CameraAuthStatus {
        switch AVCaptureDevice.authorizationStatus(for: .video) {
        case .notDetermined: return .notDetermined
        case .authorized: return .authorized
        case .denied: return .denied
        case .restricted: return .restricted
        @unknown default: return .denied
        }
    }

    func requestAccess() async -> Bool {
        await AVCaptureDevice.requestAccess(for: .video)
    }
}
