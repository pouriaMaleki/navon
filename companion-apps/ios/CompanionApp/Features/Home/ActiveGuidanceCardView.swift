import SwiftUI

struct ActiveGuidanceCardView: View {
    let onStop: () -> Void

    var body: some View {
        Button(role: .destructive, action: { onStop() }) {
            Text(T.string("home.stop"))
                .font(.headline)
                .padding(.horizontal, 24)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .padding(.bottom, 24)
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity, alignment: .trailing)
    }
}
