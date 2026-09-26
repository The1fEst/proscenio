import Quickshell
import Quickshell.Wayland

ShellRoot {
    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData
            screen: modelData
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            implicitHeight: 50
            WlrLayershell.namespace: "floor:qs"
            anchors {
                left: true
                right: true
                top: true
            }
        }
    }
}
