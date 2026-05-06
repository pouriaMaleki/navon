import Foundation
import os.log
#if canImport(ActivityKit)
import ActivityKit
#endif

/// Drives the routing-guidance Live Activity lifecycle. The coordinator is
/// the single point that decides whether an activity should exist, and it
/// translates each guidance tick into a content-state update. Real
/// ActivityKit calls live behind the `LiveActivityDriver` protocol so the
/// coordinator can be unit-tested without touching the OS-level activity
/// store.
@MainActor
protocol LiveActivityDriver: AnyObject {
    var activeRouteId: String? { get }
    @available(iOS 16.1, *)
    func start(
        attributes: RouteGuidanceActivityAttributes,
        state: RouteGuidanceActivityAttributes.ContentState
    )
    @available(iOS 16.1, *)
    func update(state: RouteGuidanceActivityAttributes.ContentState, routeId: String)
    func end(routeId: String?)
}

@MainActor
final class LiveActivityCoordinator {
    private static let log = Logger(
        subsystem: "me.fiksu.esp32map.companion.ios",
        category: "liveActivity"
    )

    private let driver: LiveActivityDriver

    init(driver: LiveActivityDriver) {
        self.driver = driver
    }

    /// Called whenever the gate inputs change (settings toggled, routing
    /// started/stopped, route id changed). Mirrors
    /// `RoutingActivityCoordinator.onSettingsOrRoutingChange`.
    func onSettingsOrRoutingChange(
        settings: CompanionSettings,
        isRouting: Bool,
        route: NormalizedRoutePackage?
    ) {
        let want = shouldRun(settings: settings, isRouting: isRouting, route: route)
        let active = driver.activeRouteId
        let routeId = route?.routeIdentifier

        if !want {
            if active != nil {
                driver.end(routeId: active)
            }
            return
        }

        // Want to run.
        guard let routeId = routeId else { return }
        if active == routeId { return }
        if active != nil {
            driver.end(routeId: active)
        }
        startActivity(
            settings: settings,
            route: route!,
            progressDistanceM: 0,
            offRoute: false,
            rerouting: false,
            arrived: false
        )
    }

    /// Called once per guidance tick from the same call site that drives
    /// the audio cue engine. Pushes a derived content-state update.
    func onGuidanceTick(
        settings: CompanionSettings,
        isRouting: Bool,
        route: NormalizedRoutePackage?,
        progressDistanceM: Double,
        offRoute: Bool,
        rerouting: Bool,
        arrived: Bool,
        isImperial: Bool,
        now: Date = Date()
    ) {
        guard shouldRun(settings: settings, isRouting: isRouting, route: route),
              let route = route,
              #available(iOS 16.1, *)
        else { return }

        // Bootstrap if a tick lands before onSettingsOrRoutingChange ran
        // (e.g. cold-start race where the GPS fix beats the gating call).
        if driver.activeRouteId != route.routeIdentifier {
            if driver.activeRouteId != nil {
                driver.end(routeId: driver.activeRouteId)
            }
            startActivity(
                settings: settings,
                route: route,
                progressDistanceM: progressDistanceM,
                offRoute: offRoute,
                rerouting: rerouting,
                arrived: arrived,
                isImperial: isImperial,
                now: now
            )
            return
        }

        guard let state = LiveActivityMapper.contentState(
            route: route,
            progressDistanceM: progressDistanceM,
            offRoute: offRoute,
            rerouting: rerouting,
            arrived: arrived,
            isImperial: isImperial,
            now: now
        ) else { return }

        driver.update(state: state, routeId: route.routeIdentifier)
    }

    // MARK: -

    private func shouldRun(
        settings: CompanionSettings,
        isRouting: Bool,
        route: NormalizedRoutePackage?
    ) -> Bool {
        settings.liveActivityEnabled
            && settings.allowBackgroundGps
            && isRouting
            && route != nil
    }

    private func startActivity(
        settings: CompanionSettings,
        route: NormalizedRoutePackage,
        progressDistanceM: Double,
        offRoute: Bool,
        rerouting: Bool,
        arrived: Bool,
        isImperial: Bool = false,
        now: Date = Date()
    ) {
        guard #available(iOS 16.1, *) else { return }
        guard let state = LiveActivityMapper.contentState(
            route: route,
            progressDistanceM: progressDistanceM,
            offRoute: offRoute,
            rerouting: rerouting,
            arrived: arrived,
            isImperial: isImperial,
            now: now
        ) else { return }
        let attributes = RouteGuidanceActivityAttributes(routeId: route.routeIdentifier)
        driver.start(attributes: attributes, state: state)
        Self.log.info("Live Activity started for routeId=\(route.routeIdentifier, privacy: .public)")
    }
}

// MARK: - Real ActivityKit driver

/// Production driver: forwards to ActivityKit. iOS 16.1+. Activities are
/// started locally (no APNs); the coordinator pushes updates each tick.
@MainActor
final class ActivityKitLiveActivityDriver: LiveActivityDriver {
    private static let log = Logger(
        subsystem: "me.fiksu.esp32map.companion.ios",
        category: "liveActivity"
    )

    private(set) var activeRouteId: String?

    #if canImport(ActivityKit)
    @available(iOS 16.1, *)
    private var activity: Activity<RouteGuidanceActivityAttributes>? {
        get { _activity as? Activity<RouteGuidanceActivityAttributes> }
        set { _activity = newValue }
    }
    private var _activity: Any?
    #endif

    init() {
        // On cold start, sweep any orphan activities left behind by a
        // previous process. iOS does not give the app a chance to run
        // code on force-quit, so an activity started in the prior session
        // is still alive when the user taps it to relaunch — but the
        // app's routing state has been wiped, so the activity has no
        // owner. End it immediately rather than leaving a phantom on the
        // lock screen until staleDate expires.
        #if canImport(ActivityKit)
        if #available(iOS 16.1, *) {
            for orphan in Activity<RouteGuidanceActivityAttributes>.activities {
                Task { await orphan.end(orphan.content, dismissalPolicy: .immediate) }
            }
        }
        #endif
    }

    @available(iOS 16.1, *)
    func start(
        attributes: RouteGuidanceActivityAttributes,
        state: RouteGuidanceActivityAttributes.ContentState
    ) {
        #if canImport(ActivityKit)
        guard ActivityAuthorizationInfo().areActivitiesEnabled else {
            Self.log.notice("Live Activities disabled in OS settings — skipping start")
            return
        }
        do {
            let activity = try Activity.request(
                attributes: attributes,
                content: .init(state: state, staleDate: nil),
                pushType: nil
            )
            self.activity = activity
            self.activeRouteId = attributes.routeId
        } catch {
            Self.log.error("Activity.request failed: \(String(describing: error), privacy: .public)")
        }
        #endif
    }

    @available(iOS 16.1, *)
    func update(state: RouteGuidanceActivityAttributes.ContentState, routeId: String) {
        #if canImport(ActivityKit)
        guard let activity = self.activity, activity.attributes.routeId == routeId else { return }
        Task { await activity.update(.init(state: state, staleDate: nil)) }
        #endif
    }

    func end(routeId: String?) {
        #if canImport(ActivityKit)
        if #available(iOS 16.1, *) {
            if let activity = self.activity {
                let final = activity.content
                Task { await activity.end(final, dismissalPolicy: .immediate) }
            }
            self.activity = nil
        }
        #endif
        self.activeRouteId = nil
    }
}
