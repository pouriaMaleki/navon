import ActivityKit
import WidgetKit
import SwiftUI

@available(iOS 16.2, *)
struct RouteGuidanceLiveActivity: Widget {
    var body: some WidgetConfiguration {
        ActivityConfiguration(for: RouteGuidanceActivityAttributes.self) { context in
            // Lock screen / banner presentation.
            LockScreenView(state: context.state)
                .padding(16)
                .activityBackgroundTint(Color.black.opacity(0.85))
                .activitySystemActionForegroundColor(.white)
        } dynamicIsland: { context in
            DynamicIsland {
                DynamicIslandExpandedRegion(.leading) {
                    ManeuverArrow(glyph: context.state.glyph)
                        .frame(width: 56, height: 56)
                }
                DynamicIslandExpandedRegion(.trailing) {
                    VStack(alignment: .trailing, spacing: 2) {
                        Text(distanceLabel(context.state.distanceToNextM, isImperial: context.state.isImperial))
                            .font(.system(size: 24, weight: .bold, design: .rounded))
                            .monospacedDigit()
                        Text(etaLabel(unixMs: context.state.etaUnixMs))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
                DynamicIslandExpandedRegion(.bottom) {
                    HStack {
                        StatusBadge(status: context.state.status)
                        Spacer()
                        Text(remainingLabel(context.state.distanceRemainingM, isImperial: context.state.isImperial))
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            } compactLeading: {
                ManeuverArrow(glyph: context.state.glyph)
                    .frame(width: 18, height: 18)
            } compactTrailing: {
                Text(distanceLabel(context.state.distanceToNextM, isImperial: context.state.isImperial))
                    .font(.caption.bold())
                    .monospacedDigit()
            } minimal: {
                ManeuverArrow(glyph: context.state.glyph)
                    .frame(width: 18, height: 18)
            }
        }
    }
}

@available(iOS 16.2, *)
private struct LockScreenView: View {
    let state: RouteGuidanceActivityAttributes.ContentState

    var body: some View {
        VStack(spacing: 8) {
            if state.status != .onRoute {
                StatusBadge(status: state.status)
                    .frame(maxWidth: .infinity, alignment: .leading)
            }
            HStack(alignment: .center, spacing: 16) {
                ManeuverArrow(glyph: state.glyph)
                    .frame(width: 88, height: 88)
                VStack(alignment: .leading, spacing: 4) {
                    Text(distanceLabel(state.distanceToNextM, isImperial: state.isImperial))
                        .font(.system(size: 44, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .lineLimit(1)
                        .minimumScaleFactor(0.6)
                    Text(
                        "\(remainingLabel(state.distanceRemainingM, isImperial: state.isImperial)) · ETA \(etaLabel(unixMs: state.etaUnixMs))"
                    )
                    .font(.subheadline)
                    .foregroundStyle(.white.opacity(0.75))
                    .monospacedDigit()
                }
                Spacer()
            }
        }
        .foregroundStyle(.white)
    }
}

private struct ManeuverArrow: View {
    let glyph: ManeuverGlyph

    var body: some View {
        Image(glyph.assetName)
            .resizable()
            .renderingMode(.template)
            .aspectRatio(contentMode: .fit)
            .foregroundStyle(.white)
    }
}

private struct StatusBadge: View {
    let status: GuidanceStatus

    var body: some View {
        if let label = label {
            Text(label)
                .font(.caption2.bold())
                .padding(.horizontal, 8)
                .padding(.vertical, 3)
                .background(badgeColor.opacity(0.85), in: Capsule())
                .foregroundStyle(.white)
        }
    }

    private var label: String? {
        switch status {
        case .onRoute:   return nil
        case .offRoute:  return "OFF ROUTE"
        case .rerouting: return "REROUTING…"
        case .arrived:   return "ARRIVED"
        }
    }

    private var badgeColor: Color {
        switch status {
        case .onRoute:   return .clear
        case .offRoute:  return .red
        case .rerouting: return .orange
        case .arrived:   return .green
        }
    }
}

// MARK: - Local formatters
//
// The widget extension can't pull in the app's i18n machinery without
// bloating the binary, so the small amount of formatting it needs lives
// here as plain functions. Mirror what the spec'd cue catalog does:
// metric → "120 m" / "3.4 km", imperial → "400 ft" / "2.1 mi".

private func distanceLabel(_ meters: Double, isImperial: Bool) -> String {
    if isImperial {
        let feet = meters * 3.280839895
        if feet < 1000 {
            return "\(Int((feet / 10).rounded()) * 10) ft"
        }
        let miles = feet / 5280
        return String(format: "%.1f mi", miles)
    } else {
        if meters < 1000 {
            return "\(Int((meters / 10).rounded()) * 10) m"
        }
        return String(format: "%.1f km", meters / 1000)
    }
}

private func remainingLabel(_ meters: Double, isImperial: Bool) -> String {
    "\(distanceLabel(meters, isImperial: isImperial)) left"
}

private func etaLabel(unixMs: UInt64) -> String {
    let date = Date(timeIntervalSince1970: TimeInterval(unixMs) / 1000.0)
    let formatter = DateFormatter()
    formatter.timeStyle = .short
    formatter.dateStyle = .none
    return formatter.string(from: date)
}
