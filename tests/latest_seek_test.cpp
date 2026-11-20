#include "LatestSeek.h"

#include <cstdlib>
#include <iostream>
#include <string_view>

void expect(bool condition, std::string_view scenario)
{
    if (!condition) {
        std::cerr << "Failed: " << scenario << '\n';
        std::exit(EXIT_FAILURE);
    }
}

int main()
{
    LatestSeek seeks;
    expect(!seeks.take(), "empty-slot");

    seeks.request(50, 1000);
    seeks.request(990, 1000);
    seeks.request(120, 1000);
    expect(seeks.take() == 120, "latest-wins");
    expect(!seeks.take(), "consume-once");

    seeks.request(-1, 1000);
    expect(seeks.take() == 0, "clamp-position");
    seeks.request(1001, 1000);
    expect(seeks.take() == 1000, "clamp-position");
    seeks.request(100, -1);
    expect(seeks.take() == 0, "clamp-position");

    seeks.request(0, 1000);
    expect(seeks.take() == 0, "zero-is-a-target");

    seeks.request(321, 1000);
    seeks.clear();
    expect(!seeks.take(), "cancel-pending");
    seeks.request(123, 1000);
    expect(seeks.take() == 123, "accept-after-cancel");
}
