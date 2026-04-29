import UIKit

/// Toggles `UIApplication.shared.isIdleTimerDisabled`. The wiring layer
/// feeds it `keepScreenOn && isRouting` so the timer is only suppressed
/// while the rider actually needs the display awake.
@MainActor
final class IdleTimerController {
    private weak var application: UIApplication?

    init(application: UIApplication = .shared) {
        self.application = application
    }

    func update(_ shouldKeepOn: Bool) {
        application?.isIdleTimerDisabled = shouldKeepOn
    }
}
