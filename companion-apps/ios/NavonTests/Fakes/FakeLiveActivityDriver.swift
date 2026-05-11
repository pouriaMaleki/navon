import Foundation
@testable import Navon

@MainActor
final class FakeLiveActivityDriver: LiveActivityDriver {
    private(set) var startedRouteIds: [String] = []
    private(set) var startedStates: [RouteGuidanceActivityAttributes.ContentState] = []
    private(set) var endedRouteIds: [String] = []
    private(set) var updates: [RouteGuidanceActivityAttributes.ContentState] = []
    private var _activeRouteId: String?

    var activeRouteId: String? { _activeRouteId }

    func start(
        attributes: RouteGuidanceActivityAttributes,
        state: RouteGuidanceActivityAttributes.ContentState
    ) {
        startedRouteIds.append(attributes.routeId)
        startedStates.append(state)
        _activeRouteId = attributes.routeId
    }

    func update(state: RouteGuidanceActivityAttributes.ContentState, routeId: String) {
        updates.append(state)
    }

    func end(routeId: String?) {
        if let id = routeId ?? _activeRouteId {
            endedRouteIds.append(id)
        }
        _activeRouteId = nil
    }
}
