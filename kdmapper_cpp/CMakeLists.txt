cmake_minimum_required(VERSION 3.20)
project(kdmapper_wrapper CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# KDMapper source directory
set(KDMAPPER_DIR "${CMAKE_CURRENT_SOURCE_DIR}/../kdmapper" CACHE PATH "Path to kdmapper source")

# Include directories
include_directories(
    ${KDMAPPER_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}
)

# Source files
set(WRAPPER_SOURCES
    kdmapper_wrapper.cpp
)

set(KDMAPPER_SOURCES
    ${KDMAPPER_DIR}/intel_driver.cpp
    ${KDMAPPER_DIR}/kdmapper.cpp
    ${KDMAPPER_DIR}/portable_executable.cpp
    ${KDMAPPER_DIR}/service.cpp
    ${KDMAPPER_DIR}/utils.cpp
)

# Create the wrapper library
add_library(kdmapper_cpp SHARED ${WRAPPER_SOURCES} ${KDMAPPER_SOURCES})

# Set output name
set_target_properties(kdmapper_cpp PROPERTIES
    OUTPUT_NAME "kdmapper_cpp"
    PREFIX ""
    SUFFIX ""
    POSITION_INDEPENDENT_CODE ON
)

# Windows specific settings
if(WIN32)
    target_compile_definitions(kdmapper_cpp PRIVATE
        _CRT_SECURE_NO_WARNINGS
        WIN32_LEAN_AND_MEAN
        NOMINMAX
    )

    # Link Windows libraries
    target_link_libraries(kdmapper_cpp
        gdi32
        gdi32full
        win32u
    )
endif()

# Install rules
install(TARGETS kdmapper_cpp
    LIBRARY DESTINATION lib
    RUNTIME DESTINATION bin
)

# Print configuration
message(STATUS "KDMapper directory: ${KDMAPPER_DIR}")
message(STATUS "Build type: ${CMAKE_BUILD_TYPE}")
