import SwiftUI

struct RerouteWaitingPillView: View {
    let delayedUntilMs: Double?
    let onRerouteNow: () -> Void

    var body: some View {
        TimelineView(.periodic(from: .now, by: 0.5)) { ctx in
            let nowMs = ctx.date.timeIntervalSince1970 * 1_000
            let secondsRemaining = max(0, Int(((delayedUntilMs ?? nowMs) - nowMs) / 1_000))
            HStack(spacing: 12) {
                Text("Waiting to reroute • \(secondsRemaining)s")
                    .font(.subheadline.weight(.bold))
                    .frame(maxWidth: .infinity, alignment: .leading)
                Button("Reroute now") {
                    onRerouteNow()
                }
                .font(.subheadline.weight(.bold))
                .padding(.horizontal, 10)
                .padding(.vertical, 4)
                .background(Color.black, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
                .foregroundStyle(Color.yellow)
            }
            .foregroundStyle(.black)
            .padding(.vertical, 6)
            .padding(.horizontal, 12)
            .background(Color.cyan, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        }
    }
}
