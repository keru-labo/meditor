#include "project.h"

#include <algorithm>
#include <cmath>
#include <fstream>
#include <chrono>
#include <regex>
#include <nlohmann/json.hpp>
#include <stdexcept>
#include <unordered_set>

#ifdef _WIN32
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#else
#include <cstdio>
#endif

namespace meditor {
namespace fs = std::filesystem;
using json = nlohmann::json;

std::string utf8(const fs::path& path)
{
    const auto bytes = path.u8string();
    return {bytes.begin(), bytes.end()};
}

fs::path fromUtf8(std::string_view value)
{
    return fs::path(std::u8string(value.begin(), value.end()));
}

static fs::path readPath(const json& value, const fs::path& directory)
{
    const auto path = fromUtf8(value.get<std::string>());
    return path.empty() ? path : (directory / path).lexically_normal();
}

static std::string writePath(const fs::path& path, const fs::path& directory)
{
    if (path.empty())
        return {};
    auto relative = path.lexically_relative(directory);
    return utf8(relative.empty() ? path : relative);
}

bool ClipRange::validFor(Time sourceDuration) const
{
    return start >= 0 && start < end && end <= sourceDuration && end <= maximumDuration;
}

bool Project::setClip(ClipRange range, Time sourceDuration)
{
    if (!range.validFor(sourceDuration))
        return false;
    clip = range;
    return true;
}

Project Project::load(const fs::path& file)
{
    std::ifstream stream(file, std::ios::binary);
    if (!stream)
        throw std::runtime_error("Could not open project");
    const auto data = json::parse(stream);
    const auto& version = data.at("version");
    if (version != 1 && version != 2)
        throw std::runtime_error("Unsupported project version");

    const auto directory = fs::absolute(file).parent_path();
    Project project;
    project.video = readPath(data.at("video"), directory);
    project.proxy = readPath(data.value("proxy", std::string{}), directory);
    if (project.video.empty())
        throw std::runtime_error("Project requires a video");
    if (version == 2) {
        const auto& range = data.at("clip");
        const auto& start = range.at("source_start_us");
        const auto& end = range.at("source_end_us");
        const auto validTimestamp = [](const json& value) {
            return value.is_number_integer() && value >= 0 && value <= maximumDuration;
        };
        if (!validTimestamp(start) || !validTimestamp(end) ||
            !project.setClip({start.get<Time>(), end.get<Time>()}, maximumDuration))
            throw std::runtime_error("Invalid clip interval");
    }

    const auto& audio = data.at("audio");
    if (!audio.is_array() || audio.size() > 32)
        throw std::runtime_error("Expected up to 32 audio tracks");
    for (const auto& entry : audio) {
        Track track{readPath(entry.at("path"), directory), entry.at("name"),
                    entry.value("offset_us", Time{0}), entry.value("gain", 1.0)};
        if (track.path.empty() || track.offset < -maximumDuration ||
            track.offset > maximumDuration || !std::isfinite(track.gain) ||
            track.gain < 0 || track.gain > 4)
            throw std::runtime_error("Invalid audio track");
        project.audio.push_back(std::move(track));
    }

    const auto& captions = data.at("captions");
    if (!captions.is_array() || captions.size() > 1'000'000)
        throw std::runtime_error("Invalid caption collection");
    project.captions.reserve(captions.size());
    std::unordered_set<std::string> ids;
    for (const auto& entry : captions) {
        Caption caption{entry.at("id"), entry.at("start_us"), entry.at("end_us"),
                        entry.at("speaker"), entry.at("text")};
        if (caption.start < 0 || caption.start >= caption.end || caption.end > maximumDuration ||
            caption.id.empty() || !ids.insert(caption.id).second)
            throw std::runtime_error("Invalid caption interval or duplicate ID");
        project.captions.push_back(std::move(caption));
    }
    std::stable_sort(project.captions.begin(), project.captions.end(),
                     [](const Caption& a, const Caption& b) { return a.start < b.start; });
    return project;
}

void Project::save(const fs::path& file) const
{
    const auto directory = fs::absolute(file).parent_path();
    json data{{"version", clip ? 2 : 1}, {"video", writePath(video, directory)},
              {"proxy", writePath(proxy, directory)}, {"audio", json::array()},
              {"captions", json::array()}};
    if (clip) {
        if (!clip->validFor(maximumDuration))
            throw std::runtime_error("Invalid clip interval");
        data["clip"] = {{"source_start_us", clip->start}, {"source_end_us", clip->end}};
    }
    for (const auto& track : audio)
        data["audio"].push_back({{"path", writePath(track.path, directory)}, {"name", track.name},
                                 {"offset_us", track.offset}, {"gain", track.gain}});
    for (const auto& caption : captions)
        data["captions"].push_back({{"id", caption.id}, {"start_us", caption.start},
                                    {"end_us", caption.end}, {"speaker", caption.speaker},
                                    {"text", caption.text}});

    const auto nonce = std::chrono::steady_clock::now().time_since_epoch().count();
    auto staging = directory / (".meditor-save-" + std::to_string(nonce));
    while (!fs::create_directory(staging))
        staging += "-";
    const auto temporary = staging / "project.json";
    try {
        std::ofstream stream(temporary, std::ios::binary);
        stream << data.dump(2) << '\n';
        stream.close();
        if (!stream)
            throw std::runtime_error("Could not write project");
#ifdef _WIN32
        if (!MoveFileExW(temporary.c_str(), file.c_str(), MOVEFILE_REPLACE_EXISTING))
#else
        if (std::rename(temporary.c_str(), file.c_str()) != 0)
#endif
            throw std::runtime_error("Could not replace project");
    } catch (...) {
        std::error_code ignored;
        fs::remove_all(staging, ignored);
        throw;
    }
    std::error_code ignored;
    fs::remove(staging, ignored);
}

std::vector<Caption> importSrt(const fs::path& file)
{
    std::ifstream source(file, std::ios::binary);
    if (!source)
        throw std::runtime_error("Cannot open SRT");
    const std::regex interval(
        R"(^([0-9]{2}):([0-5][0-9]):([0-5][0-9]),([0-9]{3})[ \t]+-->[ \t]+([0-9]{2}):([0-5][0-9]):([0-5][0-9]),([0-9]{3})(?:[ \t].*)?$)");
    std::string line;
    auto read = [&] {
        if (!std::getline(source, line))
            return false;
        if (!line.empty() && line.back() == '\r')
            line.pop_back();
        return true;
    };

    std::vector<Caption> captions;
    while (read()) {
        if (line.empty())
            continue;
        if (captions.empty() && line.starts_with("\xef\xbb\xbf"))
            line.erase(0, 3);
        if (line.empty() || line.find_first_not_of("0123456789") != std::string::npos || !read())
            throw std::runtime_error("Expected numbered UTF-8 SRT cue");
        std::smatch fields;
        if (!std::regex_match(line, fields, interval))
            throw std::runtime_error("Invalid SRT time range");
        auto timestamp = [&](int index) {
            return ((std::stoi(fields[index]) * 3600LL + std::stoi(fields[index + 1]) * 60LL +
                     std::stoi(fields[index + 2])) * 1000LL + std::stoi(fields[index + 3])) * 1000LL;
        };
        const auto start = timestamp(1);
        const auto end = timestamp(5);
        if (end <= start || end > maximumDuration)
            throw std::runtime_error("SRT cue outside project time range");

        std::string text;
        while (read() && !line.empty()) {
            if (!text.empty())
                text += '\n';
            text += line;
        }
        captions.push_back({std::to_string(captions.size() + 1), start, end, {}, std::move(text)});
        if (captions.size() > 1'000'000)
            throw std::runtime_error("Too many SRT cues");
    }
    if (source.bad())
        throw std::runtime_error("Could not read SRT");
    std::stable_sort(captions.begin(), captions.end(), [](const Caption& a, const Caption& b) {
        return a.start < b.start;
    });
    return captions;
}
}
