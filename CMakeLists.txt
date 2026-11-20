cmake_minimum_required(VERSION 3.21)
project(meditor LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

option(MEDITOR_BUILD_APP "Build the Qt Quick app" ON)
if(MEDITOR_BUILD_APP)
    find_package(Qt6 6.5 REQUIRED COMPONENTS Quick QuickControls2 Multimedia)
    qt_standard_project_setup()
    qt_add_executable(meditor
        src/main.cpp
        src/PlaybackBackend.h
        src/QtPlaybackBackend.h
        src/QtPlaybackBackend.cpp
    )
    qt_add_resources(meditor "qml" PREFIX "/" FILES qml/Main.qml)
    target_link_libraries(meditor PRIVATE Qt6::Quick Qt6::QuickControls2 Qt6::Multimedia)
endif()

include(CTest)
if(BUILD_TESTING)
    add_executable(latest_seek_test tests/latest_seek_test.cpp)
    target_include_directories(latest_seek_test PRIVATE src)
    add_test(NAME latest_seek COMMAND latest_seek_test)
endif()
