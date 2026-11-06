cmake_minimum_required(VERSION 3.21)
file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/version.txt" PCMIRROR_RELEASE_VERSION LIMIT_COUNT 1)
project(PCMirror VERSION ${PCMIRROR_RELEASE_VERSION} LANGUAGES C CXX)
enable_language(RC)
if(NOT WIN32)
  message(FATAL_ERROR "PC-Mirror currently targets Windows x64 with MSYS2 UCRT64.")
endif()
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(NO_MARCH_NATIVE ON CACHE BOOL "Portable receiver CPU baseline" FORCE)
set(USE_MDNS ON CACHE BOOL "Self-contained mDNS" FORCE)
set(USE_DNS_SD OFF CACHE BOOL "No external Bonjour dependency" FORCE)
add_subdirectory(third_party/uxplay)
find_package(Qt6 6.5 REQUIRED COMPONENTS Core Gui Widgets Network)
qt_standard_project_setup()
set(CMAKE_AUTORCC ON)
qt_add_executable(PC-Mirror WIN32
  src/main.cpp src/receiver.cpp src/receiver.h
  src/mainwindow.cpp src/mainwindow.h src/settings.cpp src/settings.h
  src/ui.cpp src/ui.h src/settingsdialog.cpp src/settingsdialog.h
  src/updater.cpp src/updater.h src/updatepolicy.h
  src/diagnostics.cpp src/diagnostics.h src/i18n.cpp src/i18n.h src/resources.qrc)
configure_file(packaging/app.rc.in "${CMAKE_BINARY_DIR}/app.rc" @ONLY)
target_sources(PC-Mirror PRIVATE "${CMAKE_BINARY_DIR}/app.rc")
add_library(Velopack SHARED IMPORTED)
set_target_properties(Velopack PROPERTIES
  IMPORTED_IMPLIB "${CMAKE_SOURCE_DIR}/third_party/velopack-sdk/lib/velopack_libc_win_x64_msvc.dll.lib"
  IMPORTED_LOCATION "${CMAKE_SOURCE_DIR}/third_party/velopack-sdk/lib/velopack_libc_win_x64_msvc.dll"
  INTERFACE_INCLUDE_DIRECTORIES "${CMAKE_SOURCE_DIR}/third_party/velopack-sdk/include")
target_link_libraries(PC-Mirror PRIVATE Qt6::Widgets Qt6::Network shell32 dwmapi Velopack)
target_compile_definitions(PC-Mirror PRIVATE PCMIRROR_VERSION="${PROJECT_VERSION}")
file(MAKE_DIRECTORY "${CMAKE_BINARY_DIR}/bin")
configure_file("${CMAKE_SOURCE_DIR}/third_party/velopack-sdk/lib/velopack_libc_win_x64_msvc.dll"
  "${CMAKE_BINARY_DIR}/bin/velopack_libc.dll" COPYONLY)
target_compile_definitions(PC-Mirror PRIVATE WIN32_LEAN_AND_MEAN NOMINMAX)
set_target_properties(PC-Mirror uxplay PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin")
enable_testing()
add_executable(ipad-layout-test tests/ipad_layout_test.cpp)
target_include_directories(ipad-layout-test PRIVATE third_party/uxplay)
add_test(NAME ipad-layout COMMAND ipad-layout-test)
add_executable(update-policy-test tests/update_policy_test.cpp)
target_include_directories(update-policy-test PRIVATE src)
add_test(NAME update-policy COMMAND update-policy-test)
add_executable(velopack-test-driver tests/velopack_driver.cpp)
target_link_libraries(velopack-test-driver PRIVATE Velopack)
set_target_properties(velopack-test-driver PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin")
add_executable(update-controller-test tests/update_controller_test.cpp src/updater.cpp src/updater.h src/i18n.cpp src/resources.qrc)
target_link_libraries(update-controller-test PRIVATE Qt6::Core Velopack)
target_include_directories(update-controller-test PRIVATE src)
target_compile_definitions(update-controller-test PRIVATE WIN32_LEAN_AND_MEAN NOMINMAX)
set_target_properties(update-controller-test PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin")
qt_add_executable(localization-test tests/localization_test.cpp
  src/mainwindow.cpp src/mainwindow.h src/receiver.cpp src/receiver.h
  src/diagnostics.cpp src/diagnostics.h src/settings.cpp src/settings.h
  src/settingsdialog.cpp src/settingsdialog.h src/ui.cpp src/ui.h
  src/updater.cpp src/updater.h src/i18n.cpp src/i18n.h src/resources.qrc)
target_include_directories(localization-test PRIVATE src)
target_link_libraries(localization-test PRIVATE Qt6::Widgets Qt6::Network shell32 dwmapi Velopack)
target_compile_definitions(localization-test PRIVATE WIN32_LEAN_AND_MEAN NOMINMAX)
set_target_properties(localization-test PROPERTIES RUNTIME_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/bin")
add_test(NAME localization COMMAND localization-test)
