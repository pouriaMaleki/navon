import AVFoundation
import SwiftUI
import UIKit

/// AVFoundation-backed `QrCaptureSession`. Wraps a single-input
/// `AVCaptureSession` whose `.qr` metadata feeds the `onScan` callback.
/// Production code path; `FakeQrCaptureSession` covers the same protocol in
/// unit tests.
final class AVFoundationQrCaptureSession: NSObject, QrCaptureSession, AVCaptureMetadataOutputObjectsDelegate {
    var onScan: ((String) -> Void)?

    let session = AVCaptureSession()
    private let metadataOutput = AVCaptureMetadataOutput()

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
            return
        }
        session.addInput(input)
        session.addOutput(metadataOutput)
        metadataOutput.setMetadataObjectsDelegate(self, queue: .main)
        metadataOutput.metadataObjectTypes = [.qr]
    }

    func start() {
        guard !session.isRunning else { return }
        Task.detached { [session] in
            session.startRunning()
        }
    }

    func tearDown() {
        if session.isRunning {
            session.stopRunning()
        }
        onScan = nil
    }

    func metadataOutput(
        _ output: AVCaptureMetadataOutput,
        didOutput metadataObjects: [AVMetadataObject],
        from connection: AVCaptureConnection
    ) {
        for object in metadataObjects {
            guard let machineReadable = object as? AVMetadataMachineReadableCodeObject,
                  machineReadable.type == .qr,
                  let raw = machineReadable.stringValue
            else { continue }
            onScan?(raw)
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
