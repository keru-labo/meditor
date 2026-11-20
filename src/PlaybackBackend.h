#pragma once

#include <QObject>
#include <QUrl>
#include <QVideoSink>

// All methods and signals run on the owning GUI thread. Positions are in milliseconds.
class PlaybackBackend : public QObject {
    Q_OBJECT
    Q_PROPERTY(qint64 position READ position NOTIFY positionChanged FINAL)
    Q_PROPERTY(qint64 duration READ duration NOTIFY durationChanged FINAL)
    Q_PROPERTY(bool seekable READ seekable NOTIFY seekableChanged FINAL)
    Q_PROPERTY(bool playing READ playing NOTIFY playingChanged FINAL)
    Q_PROPERTY(qreal volume READ volume WRITE setVolume NOTIFY volumeChanged FINAL)
    Q_PROPERTY(bool muted READ muted WRITE setMuted NOTIFY mutedChanged FINAL)
    Q_PROPERTY(QString error READ error NOTIFY errorChanged FINAL)

public:
    using QObject::QObject;

    virtual qint64 position() const = 0;
    virtual qint64 duration() const = 0;
    virtual bool seekable() const = 0;
    virtual bool playing() const = 0;
    virtual qreal volume() const = 0;
    virtual bool muted() const = 0;
    virtual QString error() const = 0;
    virtual void setVolume(qreal volume) = 0;
    virtual void setMuted(bool muted) = 0;

    Q_INVOKABLE virtual void open(const QUrl &source) = 0;
    Q_INVOKABLE virtual void play() = 0;
    Q_INVOKABLE virtual void pause() = 0;
    Q_INVOKABLE virtual void stop() = 0;
    Q_INVOKABLE virtual void seek(qint64 position) = 0;
    Q_INVOKABLE virtual void flushSeek() = 0;
    Q_INVOKABLE virtual void attachVideoSink(QVideoSink *sink) = 0;

signals:
    void positionChanged();
    void durationChanged();
    void seekableChanged();
    void playingChanged();
    void volumeChanged();
    void mutedChanged();
    void errorChanged();
};
