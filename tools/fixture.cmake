cmake_minimum_required(VERSION 3.24)
if(NOT DEFINED OUTPUT OR NOT DEFINED FIXTURE)
    message(FATAL_ERROR "Pass -DOUTPUT=<directory> and -DFIXTURE=<path to meditor_fixture>")
endif()
find_program(FFMPEG ffmpeg REQUIRED)
file(MAKE_DIRECTORY "${OUTPUT}")

function(encode)
    execute_process(COMMAND "${FFMPEG}" -hide_banner -loglevel error -y ${ARGN}
                    COMMAND_ERROR_IS_FATAL ANY)
endfunction()

# The original and proxy deliberately share a zero-based constant-frame-rate clock.
encode(-f lavfi -i "testsrc2=size=1280x720:rate=30:duration=60"
       -threads 2 -c:v libx264 -preset veryfast -g 120 -keyint_min 120 -sc_threshold 0
       "${OUTPUT}/original.mp4")
encode(-i "${OUTPUT}/original.mp4" -an -vf "scale=480:-2"
       -threads 2 -c:v libx264 -preset veryfast -g 6 -keyint_min 6 -sc_threshold 0 -bf 0
       "${OUTPUT}/proxy.mp4")
foreach(track IN ITEMS "game;220" "voice-a;440" "voice-b;660")
    list(GET track 0 name)
    list(GET track 1 tone)
    encode(-f lavfi -i "sine=frequency=${tone}:sample_rate=48000:duration=60"
           -ac 2 -c:a pcm_s16le "${OUTPUT}/${name}.wav")
endforeach()
execute_process(COMMAND "${FIXTURE}" "${OUTPUT}" COMMAND_ERROR_IS_FATAL ANY)
