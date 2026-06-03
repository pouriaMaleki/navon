import SwiftUI

/// Home-screen device-pairing chip. Only rendered when a device is paired — the unpaired
/// entry point lives in `DeviceSettingsView`.
enum DeviceChipState: Equatable {
    case connecting(name: String)
    case connected(name: String)
    case pairedDisconnected(name: String)

    var symbolName: String {
        switch self {
        case .connecting: return "bolt.horizontal.circle"
        case .connected: return "bolt.horizontal.circle.fill"
        case .pairedDisconnected: return "bolt.horizontal.circle"
        }
    }

    var accessibilityLabel: String {
        switch self {
        case .connecting(let name):
            return "Connecting to \(name)"
        case .connected(let name):
            return "Connected to \(name)"
        case .pairedDisconnected(let name):
            return "Device \(name) disconnected, tap to reconnect"
        }
    }

    var isInteractive: Bool {
        if case .connecting = self { return false }
        return true
    }
}

struct DeviceStatusChip: View {
    let state: DeviceChipState
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            ZStack {
                Image(systemName: state.symbolName)
                    .font(.system(size: 18, weight: .semibold))
                    .foregroundStyle(foreground)
                if case .connecting = state {
                    ProgressView()
                        .progressViewStyle(.circular)
                        .scaleEffect(0.8)
                }
            }
            .frame(width: 50, height: 50)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .buttonStyle(.plain)
        .disabled(!state.isInteractive)
        .accessibilityLabel(state.accessibilityLabel)
    }

    private var foreground: Color {
        switch state {
        case .connected:
            return .accentColor
        case .connecting, .pairedDisconnected:
            return .primary
        }
    }
}
