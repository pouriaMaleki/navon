import Foundation
#if canImport(ActivityKit)
import ActivityKit

/// Shared between the main app target and the widget extension. Used by
/// `Activity<RoutingLiveActivityAttributes>` so the lock-screen view and
/// the app-side updater both speak the same schema.
public struct RoutingLiveActivityAttributes: ActivityAttributes {
    public struct ContentState: Codable, Hashable {
        public var destinationLabel: String
        public var nextInstruction: String
        public var etaMinutes: Int

        public init(destinationLabel: String, nextInstruction: String, etaMinutes: Int) {
            self.destinationLabel = destinationLabel
            self.nextInstruction = nextInstruction
            self.etaMinutes = etaMinutes
        }
    }
    public var routeIdentifier: String

    public init(routeIdentifier: String) {
        self.routeIdentifier = routeIdentifier
    }
}
#endif
