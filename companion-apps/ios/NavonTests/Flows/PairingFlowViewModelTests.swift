import XCTest
@testable import Navon

@MainActor
final class PairingFlowViewModelTests: XCTestCase {

    private let validJson = #"""
    {"v":1,"id_android":"AA:BB:CC:DD:EE:FF","secret":"QkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkJCQkI=","fw":"0.1.0"}
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
        // The VM gates `handleScannedQr` on `pairingState == .scanning`
        // (dedup against AVFoundation's repeated metadata callbacks), so
        // we must enter the scanning step before simulating a scan.
        await vm.enterScanningStep()

        session.simulateScan(validJson)
        // `handleScannedQr` is dispatched via a spawned `Task { @MainActor }`;
        // a single `Task.yield()` doesn't reliably flush that hop. Sleep a
        // tick so the spawned task runs before we assert.
        try? await Task.sleep(nanoseconds: 50_000_000)

        XCTAssertNotNil(vm.lastDecodedPayload)
        // The decoded secret is what gets written to `…1004` next; assert
        // on it (and not on `id_android`) because iOS uses only the secret.
        XCTAssertEqual(vm.lastDecodedPayload?.ephemeralSecret, Data(repeating: 0x42, count: 32))
        // After a successful scan the VM moves out of `.scanning`. `completePairing`
        // races forward through `.confirming` → `.succeeded` → `.idle` so we
        // accept anything that isn't `.scanning` (or `.instructions`).
        XCTAssertNotEqual(vm.pairingState, .scanning)
        XCTAssertNotEqual(vm.pairingState, .instructions)
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
