import SwiftUI

struct PhoneGuidanceTopCardView: View {
    let headline: String
    let distanceLine: String
    let minutesLine: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(headline)
                .font(.headline)
                .lineLimit(2)
            if !distanceLine.isEmpty {
                Text(distanceLine)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if !minutesLine.isEmpty {
                Text(minutesLine)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }
}
