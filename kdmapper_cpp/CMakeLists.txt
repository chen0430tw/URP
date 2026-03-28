cmake_minimum_required(VERSION 3.20)
project(kdmapper_wrapper CXX)

set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# KDMapper source directory (absolute path since kdmapper is outside URP)
set(KDMAPPER_DIR "C:/Users/Administrator/kdmapper/kdmapper" CACHE PATH "Path to kdmapper source")

# Include directories
include_directories(
    ${KDMAPPER_DIR}
    ${KDMAPPER_DIR}/include
    ${CMAKE_CURRENT_SOURCE_DIR}
)

# Source files
set(WRAPPER_SOURCES
    kdmapper_wrapper.cpp
)

set(KDMAPPER_SOURCES
    ${KDMAPPER_DIR}/intel_driver.cpp
    ${KDMAPPER_DIR}/kdmapper.cpp
    ${KDMAPPER_DIR}/KDSymbolsHandler.cpp
    ${KDMAPPER_DIR}/portable_executable.cpp
    ${KDMAPPER_DIR}/service.cpp
    ${KDMAPPER_DIR}/utils.cpp
)

# Create the wrapper library
add_library(kdmapper_cpp SHARED ${WRAPPER_SOURCES} ${KDMAPPER_SOURCES})

# Set output name - keep default extensions for Windows
set_target_properties(kdmapper_cpp PROPERTIES
    OUTPUT_NAME "kdmapper_cpp"
    POSITION_INDEPENDENT_CODE ON
)

# Windows specific settings
if(WIN32)
    target_compile_definitions(kdmapper_cpp PRIVATE
        _CRT_SECURE_NO_WARNINGS
        KDLIBMODE
        _UNICODE
        UNICODE
        DISABLE_OUTPUT
    )

    # Link Windows libraries
    # Note: gdi32full and win32u are system DLLs loaded dynamically
    target_link_libraries(kdmapper_cpp
        gdi32
        kernel32
        user32
        advapi32
        shell32
        ntdll
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
