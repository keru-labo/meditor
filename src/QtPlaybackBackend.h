#pragma once

#include "LatestSeek.h"
#include "PlaybackBackend.h"

#include <QAudioOutput>
#include <QMediaPlayer>
#include <QTimer>

class QtPlaybackBackend final : public PlaybackBackend {
public:
    QtPlaybackBackend();

    qint64 position() const override { return player_.position(); }
    qint64 duration() const override { return player_.duration(); }
    bool seekable() const override { return player_.isSeekable(); }
    bool playing() const override { return player_.playbackState() == QMediaPlayer::PlayingState; }
    qreal volume() const override { return audio_.volume(); }
    bool muted() const override { return audio_.isMuted(); }
    QString error() const override { return player_.errorString(); }
    void setVolume(qreal volume) override { audio_.setVolume(volume); }
    void setMuted(bool muted) override { audio_.setMuted(muted); }

    void open(const QUrl &source) override;
    void play() override;
    void pause() override { player_.pause(); }
    void stop() override;
    void seek(qint64 position) override;
    void flushSeek() override;
    void attachVideoSink(QVideoSink *sink) override { player_.setVideoSink(sink); }

private:
    void cancelSeek();

    QAudioOutput audio_;
    QMediaPlayer player_;
    QTimer seekTimer_;
    LatestSeek pendingSeek_;
};
