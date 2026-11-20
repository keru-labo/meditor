#include "QtPlaybackBackend.h"

QtPlaybackBackend::QtPlaybackBackend()
{
    player_.setAudioOutput(&audio_);
    audio_.setVolume(0.7f);
    seekTimer_.setSingleShot(true);
    seekTimer_.setTimerType(Qt::PreciseTimer);
    seekTimer_.setInterval(16);

    connect(&seekTimer_, &QTimer::timeout, this, &QtPlaybackBackend::flushSeek);
    connect(&player_, &QMediaPlayer::positionChanged, this, &PlaybackBackend::positionChanged);
    connect(&player_, &QMediaPlayer::durationChanged, this, &PlaybackBackend::durationChanged);
    connect(&player_, &QMediaPlayer::seekableChanged, this, &PlaybackBackend::seekableChanged);
    connect(&player_, &QMediaPlayer::playbackStateChanged, this, &PlaybackBackend::playingChanged);
    connect(&player_, &QMediaPlayer::errorChanged, this, &QtPlaybackBackend::cancelSeek);
    connect(&player_, &QMediaPlayer::errorChanged, this, &PlaybackBackend::errorChanged);
    connect(&audio_, &QAudioOutput::volumeChanged, this, &PlaybackBackend::volumeChanged);
    connect(&audio_, &QAudioOutput::mutedChanged, this, &PlaybackBackend::mutedChanged);
}

void QtPlaybackBackend::open(const QUrl &source)
{
    cancelSeek();
    player_.setSource(source);
}

void QtPlaybackBackend::play()
{
    flushSeek();
    player_.play();
}

void QtPlaybackBackend::stop()
{
    cancelSeek();
    player_.stop();
}

void QtPlaybackBackend::seek(qint64 position)
{
    if (!seekable())
        return;

    pendingSeek_.request(position, duration());
    if (!seekTimer_.isActive())
        seekTimer_.start();
}

void QtPlaybackBackend::flushSeek()
{
    seekTimer_.stop();
    const auto target = pendingSeek_.take();
    if (!target || !seekable())
        return;

    // The FFmpeg backend produces seek frames in PausedState.
    if (player_.playbackState() == QMediaPlayer::StoppedState)
        player_.pause();
    player_.setPosition(std::clamp<qint64>(*target, 0, duration()));
}

void QtPlaybackBackend::cancelSeek()
{
    seekTimer_.stop();
    pendingSeek_.clear();
}
