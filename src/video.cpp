#include "video.h"

extern "C" {
#include <libavcodec/avcodec.h>
#include <libavformat/avformat.h>
#include <libswscale/swscale.h>
}

#include <algorithm>
#include <atomic>
#include <condition_variable>
#include <cerrno>
#include <mutex>
#include <stdexcept>
#include <thread>
#include <utility>

namespace meditor {
namespace {
void check(int code, const char* operation)
{
    if (code >= 0)
        return;
    char message[AV_ERROR_MAX_STRING_SIZE]{};
    av_strerror(code, message, sizeof(message));
    throw std::runtime_error(std::string(operation) + ": " + message);
}

struct Decoder {
    AVFormatContext* format = nullptr;
    AVCodecContext* codec = nullptr;
    AVPacket* packet = nullptr;
    AVFrame* decoded = nullptr;
    AVFrame* previous = nullptr;
    Time previousPts = 0;
    SwsContext* converter = nullptr;
    AVRational timeBase{};
    int stream = 0;
    Time origin = 0;
    Time duration = 0;
    Time lastRequest = -1;
    std::optional<Time> lastSourceEnd;
    bool reachedEnd = false;
    bool eof = false;
    std::shared_ptr<const VideoFrame> shown;

    explicit Decoder(const std::filesystem::path& path)
    {
        try {
            AVDictionary* options = nullptr;
            av_dict_set(&options, "protocol_whitelist", "file", 0);
            const int opened = avformat_open_input(&format, utf8(path).c_str(), nullptr, &options);
            av_dict_free(&options);
            check(opened, "Open video");
            check(avformat_find_stream_info(format, nullptr), "Read video streams");
            stream = av_find_best_stream(format, AVMEDIA_TYPE_VIDEO, -1, -1, nullptr, 0);
            check(stream, "Find video stream");
            const auto* info = format->streams[stream];
            timeBase = info->time_base;
            origin = info->start_time == AV_NOPTS_VALUE ? 0 : info->start_time;
            duration = info->duration == AV_NOPTS_VALUE
                ? format->duration : av_rescale_q(info->duration, timeBase, AV_TIME_BASE_Q);
            if (duration <= 0 || duration > maximumDuration)
                throw std::runtime_error("Video needs a known duration of at most 24 hours");

            auto* implementation = avcodec_find_decoder(info->codecpar->codec_id);
            if (!implementation)
                throw std::runtime_error("No FFmpeg decoder for this video");
            codec = avcodec_alloc_context3(implementation);
            packet = av_packet_alloc();
            decoded = av_frame_alloc();
            previous = av_frame_alloc();
            if (!codec || !packet || !decoded || !previous)
                throw std::bad_alloc();
            check(avcodec_parameters_to_context(codec, info->codecpar), "Read codec parameters");
            codec->thread_count = 2;
            codec->thread_type = FF_THREAD_SLICE;
            check(avcodec_open2(codec, implementation, nullptr), "Open decoder");
        } catch (...) {
            release();
            throw;
        }
    }

    ~Decoder() { release(); }
    Decoder(const Decoder&) = delete;
    Decoder& operator=(const Decoder&) = delete;

    void release()
    {
        sws_freeContext(converter);
        av_frame_free(&previous);
        av_frame_free(&decoded);
        av_packet_free(&packet);
        avcodec_free_context(&codec);
        avformat_close_input(&format);
    }

    bool decodeOne()
    {
        while (true) {
            int code = avcodec_receive_frame(codec, decoded);
            if (code == 0)
                return true;
            if (code == AVERROR_EOF)
                return false;
            if (code != AVERROR(EAGAIN))
                check(code, "Decode frame");

            if (eof)
                return false;
            do {
                av_packet_unref(packet);
                code = av_read_frame(format, packet);
            } while (code >= 0 && packet->stream_index != stream);
            if (code == AVERROR_EOF) {
                eof = true;
                check(avcodec_send_packet(codec, nullptr), "Drain decoder");
            } else {
                check(code, "Read packet");
                check(avcodec_send_packet(codec, packet), "Submit packet");
                av_packet_unref(packet);
            }
        }
    }

    std::shared_ptr<const VideoFrame> convert(const AVFrame& image, Time pts)
    {
        const double scale = std::min({1.0, 960.0 / image.width, 540.0 / image.height});
        const int width = std::max(1, static_cast<int>(image.width * scale));
        const int height = std::max(1, static_cast<int>(image.height * scale));
        const int stride = (width * 3 + 63) & ~63;
        converter = sws_getCachedContext(converter, image.width, image.height,
            static_cast<AVPixelFormat>(image.format), width, height, AV_PIX_FMT_RGB24,
            SWS_FAST_BILINEAR, nullptr, nullptr, nullptr);
        if (!converter)
            throw std::runtime_error("Could not convert decoded frame");
        auto frame = std::make_shared<VideoFrame>(VideoFrame{pts, width, height, stride, {}});
        frame->rgb.resize(static_cast<std::size_t>(stride) * height + 64);
        unsigned char* output[] = {frame->rgb.data(), nullptr, nullptr, nullptr};
        const int strides[] = {stride, 0, 0, 0};
        if (sws_scale(converter, image.data, image.linesize, 0, image.height,
                      output, strides) != height)
            throw std::runtime_error("Incomplete frame conversion");
        return frame;
    }

    std::shared_ptr<const VideoFrame> at(Time target, std::optional<Time> sourceEnd, const auto& cancelled)
    {
        const Time limit = std::min(duration, sourceEnd.value_or(duration));
        if (limit <= 0)
            return {};
        target = std::clamp(target, Time{0}, limit - 1);
        if (lastRequest < 0 || sourceEnd != lastSourceEnd || target < lastRequest || target - lastRequest > 300'000) {
            const auto ticks = sourceEnd
                ? av_rescale_q_rnd(target, AV_TIME_BASE_Q, timeBase, AV_ROUND_DOWN)
                : av_rescale_q(target, AV_TIME_BASE_Q, timeBase);
            const auto timestamp = origin + ticks;
            check(av_seek_frame(format, stream, timestamp, AVSEEK_FLAG_BACKWARD), "Seek video");
            avcodec_flush_buffers(codec);
            av_frame_unref(decoded);
            av_frame_unref(previous);
            shown.reset();
            reachedEnd = false;
            eof = false;
        }
        lastRequest = target;
        lastSourceEnd = sourceEnd;
        if (shown && shown->pts >= target)
            return shown;

        while (!cancelled() && !reachedEnd && decodeOne()) {
            const auto timestamp = decoded->best_effort_timestamp;
            if (timestamp == AV_NOPTS_VALUE)
                continue;
            const auto pts = av_rescale_q(timestamp - origin, timeBase, AV_TIME_BASE_Q);
            if (sourceEnd && pts >= limit) {
                reachedEnd = true;
                break;
            }
            if (pts >= target) {
                shown = convert(*decoded, pts);
                return shown;
            }
            av_frame_unref(previous);
            av_frame_move_ref(previous, decoded);
            previousPts = pts;
        }

        if (cancelled()) {
            lastRequest = -1;
            return {};
        }
        if (previous->width > 0 && (!shown || previousPts > shown->pts))
            shown = convert(*previous, previousPts);
        return shown;
    }
};
}

struct VideoWorker::State {
    explicit State(const std::filesystem::path& source, std::filesystem::path proxy)
        : original(source), proxyPath(std::move(proxy)), worker([this](std::stop_token stop) {
            run(stop);
        }) {}

    void run(std::stop_token stop)
    {
        std::unique_ptr<Decoder> proxy;
        while (!stop.stop_requested()) {
            std::unique_lock lock(mutex);
            if (!ready.wait(lock, stop, [this] { return pending.has_value(); }))
                return;
            const auto request = *std::exchange(pending, std::nullopt);
            const auto ticket = revision.load();
            lock.unlock();

            auto cancelled = [&] {
                return stop.stop_requested() || epoch.load() != request.epoch ||
                    (request.scrub && revision.load() != ticket);
            };
            Preview result{request, {}, {}};
            try {
                if (request.proxy && !proxy) {
                    auto candidate = std::make_unique<Decoder>(proxyPath);
                    if (std::abs(candidate->duration - original.duration) > 100'000)
                        throw std::runtime_error("Proxy duration differs from original");
                    proxy = std::move(candidate);
                }
                auto& decoder = request.proxy ? *proxy : original;
                result.frame = decoder.at(request.target, request.sourceEnd, cancelled);
                if (!result.frame && !cancelled())
                    result.error = "No frame found at the requested time";
            } catch (const std::exception& error) {
                result.error = error.what();
            }

            lock.lock();
            if (!cancelled())
                completed = std::move(result);
        }
    }

    Decoder original;
    std::filesystem::path proxyPath;
    std::mutex mutex;
    std::condition_variable_any ready;
    std::optional<VideoRequest> pending;
    std::optional<Preview> completed;
    std::atomic<std::uint64_t> epoch{0};
    std::atomic<std::uint64_t> revision{0};
    std::jthread worker; // Joins before decoders and queues are destroyed.
};

VideoWorker::VideoWorker(const std::filesystem::path& original, const std::filesystem::path& proxy)
    : state_(std::make_unique<State>(original, proxy)) {}
VideoWorker::~VideoWorker() = default;
Time VideoWorker::duration() const { return state_->original.duration; }

void VideoWorker::submit(VideoRequest request)
{
    std::lock_guard lock(state_->mutex);
    if (request.scrub || request.epoch != state_->epoch.load())
        state_->completed.reset();
    state_->epoch = request.epoch;
    ++state_->revision;
    state_->pending = request;
    state_->ready.notify_one();
}

std::optional<Preview> VideoWorker::take()
{
    std::lock_guard lock(state_->mutex);
    return std::exchange(state_->completed, std::nullopt);
}
}
