import Foundation
import SwiftUI

/// Test seam for the AVFoundation QR pipeline. Production wraps an
/// `AVCaptureSession` + metadata output; tests use a fake that lets the
/// suite simulate scans synchronously.
protocol QrCaptureSession: AnyObject {
    var onScan: ((String) -> Void)? { get set }
    func tearDown()
}

/// Camera permission abstraction so unit tests can drive the
/// "denied → Open Settings" path without poking AVFoundation.
protocol CameraPermissionProvider {
    func currentStatus() -> CameraAuthStatus
    func requestAccess() async -> Bool
}

enum CameraAuthStatus: Equatable {
    case notDetermined
    case authorized
    case denied
    case restricted
}

@MainActor
final class PairingFlowViewModel: ObservableObject {
    enum Step: Equatable {
        case instructions
        case scanning
        case connecting
        case succeeded
        case failed(String)
    }

    enum DeniedAction: Equatable { case openSettings }

    enum PermissionDescriptor: Equatable {
        case notDetermined
        case authorized
        case denied(DeniedAction)
    }

    enum ScanGuidance: Equatable {
        case none
        case centerOnQr
    }

    @Published var pairingState: Step = .instructions
    @Published private(set) var lastDecodedPayload: PairingQrPayload?
    @Published var scanErrorMessage: String?
    @Published var scanGuidance: ScanGuidance = .none
    @Published var permissionDescriptor: PermissionDescriptor = .notDetermined

    private let session: QrCaptureSession
    private let permission: CameraPermissionProvider
    private weak var appModel: AppModel?
    private var consecutiveInvalidScans: Int = 0

    init(
        session: QrCaptureSession,
        permission: CameraPermissionProvider,
        appModel: AppModel?
    ) {
        self.session = session
        self.permission = permission
        self.appModel = appModel
        self.session.onScan = { [weak self] raw in
            Task { @MainActor [weak self] in
                self?.handleScannedQr(raw)
            }
        }
    }

    func enterScanningStep() async {
        // Tell the device to show its QR before opening the camera.
        // The device defaults to the map; without this write the
        // panel stays on the map and there's nothing to scan.
        if let appModel {
            do {
                try await appModel.prepareDeviceForPairing()
            } catch {
                pairingState = .failed(error.localizedDescription)
                return
            }
        }
        pairingState = .scanning
        appModel?.pairingState = .scanning
        await refreshPermissionDescriptor()
    }

    func cancel() {
        session.tearDown()
        appModel?.pairingState = .idle
        pairingState = .instructions
    }

    func handleScannedQr(_ raw: String) {
        // AVFoundation re-fires the metadata callback for every camera
        // frame the QR is in view of (~30 Hz). Without this gate, we'd
        // launch a fresh `completePairing` task for every frame —
        // multiple in-flight BLE writes pile up, the iOS write
        // continuation slot gets overwritten, and Swift logs
        // "leaked its continuation". Only the first successful decode
        // matters; ignore everything that arrives after we've already
        // moved past the scanning step.
        guard pairingState == .scanning else {
            return
        }
        do {
            let payload = try PairingQrPayload.decode(raw)
            lastDecodedPayload = payload
            scanErrorMessage = nil
            scanGuidance = .none
            consecutiveInvalidScans = 0
            pairingState = .connecting
            appModel?.pairingState = .connecting
            session.tearDown()
            if let appModel {
                Task { await appModel.completePairing(payload: payload) }
            }
        } catch let error as PairingQrError {
            registerInvalidScan(message: error.errorDescription ?? "Pairing QR could not be read.")
        } catch {
            registerInvalidScan(message: error.localizedDescription)
        }
    }

    private func registerInvalidScan(message: String) {
        consecutiveInvalidScans += 1
        scanErrorMessage = message
        if consecutiveInvalidScans >= 3 {
            scanGuidance = .centerOnQr
        }
    }

    private func refreshPermissionDescriptor() async {
        switch permission.currentStatus() {
        case .authorized:
            permissionDescriptor = .authorized
        case .notDetermined:
            permissionDescriptor = .notDetermined
            let granted = await permission.requestAccess()
            permissionDescriptor = granted ? .authorized : .denied(.openSettings)
        case .denied, .restricted:
            permissionDescriptor = .denied(.openSettings)
        }
    }
}
