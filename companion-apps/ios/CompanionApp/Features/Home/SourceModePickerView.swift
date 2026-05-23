import SwiftUI

struct SourceModePickerView: View {
    let modes: [RouteSourceMode]
    let currentMode: RouteSourceMode
    let onSelect: (RouteSourceMode) -> Void

    var body: some View {
        HStack(spacing: 8) {
            ForEach(modes) { mode in
                Button {
                    onSelect(mode)
                } label: {
                    Text(mode.displayName)
                        .font(.subheadline.weight(.semibold))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .frame(maxWidth: .infinity)
                        .background(
                            currentMode == mode ? Color.blue.opacity(0.16) : Color.clear,
                            in: RoundedRectangle(cornerRadius: 14, style: .continuous)
                        )
                }
                .buttonStyle(.plain)
            }
        }
        .padding(8)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }
}
