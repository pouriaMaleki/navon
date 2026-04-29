import Foundation
#if canImport(ActivityKit)
import ActivityKit
#endif

/// ActivityKit-backed Live Activity for the lock-screen route status. Wraps
/// the boilerplate of starting/updating/ending an Activity around a typed
/// attribute. Lives behind the [LiveActivityPort] so tests don't need
/// ActivityKit.
///
/// The widget views themselves live in the `RoutingLiveActivityWidget`
/// target (project.yml); the shared attribute lives in
/// `CompanionLiveActivityShared/` so both targets agree on the schema.

#if canImport(ActivityKit)
@available(iOS 16.2, *)
final class ActivityKitLiveActivityService: LiveActivityPort {
    private var activity: Activity<RoutingLiveActivityAttributes>?

    func start(_ content: RoutingLiveActivityContent) {
        guard activity == nil else { return }
        guard ActivityAuthorizationInfo().areActivitiesEnabled else { return }
        let attrs = RoutingLiveActivityAttributes(routeIdentifier: content.routeIdentifier)
        let state = RoutingLiveActivityAttributes.ContentState(
            destinationLabel: content.destinationLabel,
            nextInstruction: content.nextInstruction,
            etaMinutes: content.etaMinutes
        )
        do {
            if #available(iOS 16.2, *) {
                activity = try Activity<RoutingLiveActivityAttributes>.request(
                    attributes: attrs,
                    content: .init(state: state, staleDate: nil),
                    pushType: nil
                )
            }
        } catch {
            activity = nil
        }
    }

    func update(_ content: RoutingLiveActivityContent) {
        guard let activity else { return }
        let state = RoutingLiveActivityAttributes.ContentState(
            destinationLabel: content.destinationLabel,
            nextInstruction: content.nextInstruction,
            etaMinutes: content.etaMinutes
        )
        Task {
            if #available(iOS 16.2, *) {
                await activity.update(.init(state: state, staleDate: nil))
            }
        }
    }

    func end() {
        guard let activity else { return }
        Task {
            if #available(iOS 16.2, *) {
                await activity.end(nil, dismissalPolicy: .immediate)
            }
        }
        self.activity = nil
    }

    /// End every routing activity that is still alive in iOS — covers
    /// the case where the user force-swiped the app while a ride was in
    /// progress. ActivityKit otherwise keeps the activity displayed
    /// (because that's the use case it's designed for, e.g. delivery
    /// trackers); we don't want a phantom routing card sitting on the
    /// lock screen after the rider obviously stopped using the app.
    func endAllOutstanding() {
        if #available(iOS 16.2, *) {
            for activity in Activity<RoutingLiveActivityAttributes>.activities {
                Task {
                    await activity.end(nil, dismissalPolicy: .immediate)
                }
            }
        }
        self.activity = nil
    }
}
#endif
