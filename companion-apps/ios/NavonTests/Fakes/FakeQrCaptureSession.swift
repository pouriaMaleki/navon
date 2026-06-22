import Foundation
@testable import Navon

@MainActor
final class FakeQrCaptureSession: QrCaptureSession {
    var onScan: ((String) -> Void)?
    private(set) var tearDownCallCount = 0

    func simulateScan(_ rawString: String) {
        onScan?(rawString)
    }

    func tearDown() {
        tearDownCallCount += 1
    }
}

final class FakeCameraPermissionProvider: CameraPermissionProvider {
    var initialStatus: CameraAuthStatus
    var requestAccessResult: Bool

    init(initialStatus: CameraAuthStatus = .authorized, requestAccessResult: Bool = true) {
        self.initialStatus = initialStatus
        self.requestAccessResult = requestAccessResult
    }

    func currentStatus() -> CameraAuthStatus {
        initialStatus
    }

    func requestAccess() async -> Bool {
        requestAccessResult
    }
}
