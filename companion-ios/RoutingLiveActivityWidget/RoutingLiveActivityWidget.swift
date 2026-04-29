import ActivityKit
import SwiftUI
import WidgetKit

/// Lock-screen + Dynamic Island Live Activity for the routing flow.
/// Triggered by the main app when `liveActivityEnabled && allowBackgroundGps`
/// and a route is active.
struct RoutingLiveActivityWidget: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: RoutingLiveActivityAttributes.self) { context in
            // Lock-screen presentation.
            VStack(alignment: .leading, spacing: 6) {
                Text(context.state.destinationLabel)
                    .font(.headline)
                Text(context.state.nextInstruction)
                    .font(.subheadline)
                    .foregroundColor(.secondary)
                Text("\(context.state.etaMinutes) min")
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            .padding()
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    Text(context.state.destinationLabel)
                        .font(.subheadline)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    Text("\(context.state.etaMinutes) min")
                        .font(.caption)
                }
                DynamicIslandExpandedRegion(.bottom) {
                    Text(context.state.nextInstruction)
                        .font(.body)
                }
            } compactLeading: {
                Image(systemName: "bicycle")
            } compactTrailing: {
                Text("\(context.state.etaMinutes)m")
                    .font(.caption2)
            } minimal: {
                Image(systemName: "bicycle")
            }
        }
    }
}
