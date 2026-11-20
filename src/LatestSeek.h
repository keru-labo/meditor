#pragma once

#include <algorithm>
#include <cstdint>
#include <optional>
#include <utility>

class LatestSeek {
public:
    using Position = std::int64_t;

    void request(Position position, Position duration)
    {
        pending_ = std::clamp(position, Position{0}, std::max(Position{0}, duration));
    }

    std::optional<Position> take()
    {
        return std::exchange(pending_, std::nullopt);
    }

    void clear() { pending_.reset(); }

private:
    std::optional<Position> pending_;
};
