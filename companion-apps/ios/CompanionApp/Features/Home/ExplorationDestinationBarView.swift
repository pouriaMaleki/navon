import SwiftUI

struct ExplorationDestinationBarView: View {
    let query: String
    let navigationTitle: String

    var body: some View {
        HStack(spacing: 12) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            let dest = query.isEmpty ? navigationTitle : query
            Text(dest)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .padding(14)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }
}
