import QtQuick
import QtQuick.Controls
import QtQuick.Dialogs
import QtQuick.Layouts
import QtMultimedia

ApplicationWindow {
    id: root
    width: 1000
    height: 660
    minimumWidth: 560
    minimumHeight: 380
    visible: true
    title: "meditor"

    FileDialog {
        id: mediaDialog
        title: qsTr("Open video")
        onAccepted: playback.open(selectedFile)
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.margins: 16
        spacing: 12

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            color: "#15171b"
            VideoOutput {
                anchors.fill: parent
                Component.onCompleted: playback.attachVideoSink(videoSink)
            }
        }

        Label {
            Layout.fillWidth: true
            visible: text.length > 0
            text: playback.error
            wrapMode: Text.Wrap
        }

        Slider {
            id: timeline
            objectName: "timeline"
            Layout.fillWidth: true
            enabled: playback.seekable && playback.duration > 0
            from: 0
            to: Math.max(1, playback.duration)
            onMoved: playback.seek(Math.round(value))
            onPressedChanged: {
                if (!pressed)
                    playback.flushSeek()
            }
            Accessible.name: qsTr("Timeline position")

            Binding {
                target: timeline
                property: "value"
                value: playback.position
                when: !timeline.pressed
                restoreMode: Binding.RestoreNone
            }
        }

        RowLayout {
            Layout.fillWidth: true

            Button {
                text: qsTr("Open video")
                onClicked: mediaDialog.open()
            }
            Button {
                text: playback.playing ? qsTr("Pause") : qsTr("Play")
                onClicked: playback.playing ? playback.pause() : playback.play()
            }
            Button {
                text: qsTr("Stop")
                onClicked: playback.stop()
            }
            Label {
                text: qsTr("%1 / %2 s").arg((playback.position / 1000).toFixed(2))
                                          .arg((playback.duration / 1000).toFixed(2))
                Layout.fillWidth: true
            }
            CheckBox {
                text: qsTr("Mute")
                checked: playback.muted
                onToggled: playback.muted = checked
            }
            Slider {
                from: 0
                to: 1
                value: playback.volume
                onMoved: playback.volume = value
                Accessible.name: qsTr("Volume")
            }
        }
    }
}
