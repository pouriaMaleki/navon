import XCTest
@testable import ESP32MapCompanion

@MainActor
final class PairingFlowViewModelTests: XCTestCase {

    private let validJson = #"""
    {
      "v": 1,
      "id_ios": "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A",
      "id_android": "AA:BB:CC:DD:EE:FF",
      "secret": "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=",
      "fw": "1.2.3"
    }
    """#

    private func makeAppModel() -> AppModel {
        let persistence = CompanionPersistence(defaults: UserDefaults(suiteName: "pairing-vm-tests-\(UUID().uuidString)")!)
        let fake = FakeRouteSyncBluetoothClient()
        let service = BleRouteSyncService(bluetoothClient: fake)
        return AppModel(persistence: persistence, bleService: service)
    }

    func test_qrCallback_validPayload_advancesPairingStateToConnecting() async {
        let session = FakeQrCaptureSession()
        let permission = FakeCameraPermissionProvider()
        let appModel = makeAppModel()
        let vm = PairingFlowViewModel(session: session, permission: permission, appModel: appModel)

        session.simulateScan(validJson)
        await Task.yield()

        XCTAssertEqual(vm.pairingState, .connecting)
        XCTAssertEqual(vm.lastDecodedPayload?.peripheralIdentifier, "B6C8AE6A-1A8C-4F2E-9F1A-9D2B0C3E4F5A")
    }

    func test_qrCallback_invalidPayload_setsHumanReadableError() async {
        let session = FakeQrCaptureSession()
        let permission = FakeCameraPermissionProvider()
        let vm = PairingFlowViewModel(session: session, permission: permission, appModel: makeAppModel())
        await vm.enterScanningStep()

        session.simulateScan("garbage")
        await Task.yield()

        XCTAssertEqual(vm.pairingState, .scanning)
        XCTAssertNotNil(vm.scanErrorMessage)
        XCTAssertFalse(vm.scanErrorMessage?.isEmpty ?? true)
    }

    func test_threeConsecutiveInvalidScans_promptCenterOnQr() async {
        let session = FakeQrCaptureSession()
        let permission = FakeCameraPermissionProvider()
        let vm = PairingFlowViewModel(session: session, permission: permission, appModel: makeAppModel())
        await vm.enterScanningStep()

        for _ in 0..<3 {
            session.simulateScan("garbage")
            await Task.yield()
        }

        XCTAssertEqual(vm.scanGuidance, .centerOnQr)
    }

    func test_cancel_inAnyStep_returnsToIdleAndTearsDownSession() async {
        for step in [PairingFlowViewModel.Step.instructions, .scanning, .connecting] {
            let session = FakeQrCaptureSession()
            let permission = FakeCameraPermissionProvider()
            let appModel = makeAppModel()
            appModel.pairingState = .scanning
            let vm = PairingFlowViewModel(session: session, permission: permission, appModel: appModel)
            vm.pairingState = step

            vm.cancel()

            XCTAssertEqual(appModel.pairingState, .idle, "Cancel from \(step) should reset AppModel.pairingState")
            XCTAssertEqual(session.tearDownCallCount, 1, "Cancel from \(step) should tear down the session exactly once")
        }
    }

    func test_cameraPermissionDenied_showsOpenSettingsAction() async {
        let session = FakeQrCaptureSession()
        let permission = FakeCameraPermissionProvider(initialStatus: .denied)
        let vm = PairingFlowViewModel(session: session, permission: permission, appModel: makeAppModel())

        await vm.enterScanningStep()

        XCTAssertEqual(vm.permissionDescriptor, .denied(.openSettings))
    }
}
