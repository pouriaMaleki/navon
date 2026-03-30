import SwiftUI

struct RootTabView: View {
    var body: some View {
        TabView {
            LaunchView()
                .tabItem { Label("Launch", systemImage: "bolt.horizontal.circle") }
            RoutePlanningView()
                .tabItem { Label("Plan", systemImage: "map") }
            RoutePreviewView()
                .tabItem { Label("Preview", systemImage: "point.topleft.down.curvedto.point.bottomright.up") }
            DeviceView()
                .tabItem { Label("Device", systemImage: "antenna.radiowaves.left.and.right") }
            ActiveRideView()
                .tabItem { Label("Ride", systemImage: "figure.outdoor.cycle") }
            SettingsView()
                .tabItem { Label("Settings", systemImage: "gearshape") }
        }
    }
}
