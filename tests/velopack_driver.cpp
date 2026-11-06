// SDK integration fixture; no application UI or custom updater implementation.
#include <Velopack.hpp>
#include <fstream>
#include <iostream>
#include <memory>
#include <stdexcept>

int main(int argc, char **argv) {
    Velopack::VelopackApp::Build().SetAutoApplyOnStartup(false).Run();
    if(argc < 4) return 0;
    std::string mode=argv[1], feed=argv[2], report=argv[3];
    try {
        Velopack::UpdateManager manager(std::make_unique<Velopack::FileSource>(feed));
        if(mode=="--verify") {
            std::ofstream output(report); output << manager.GetCurrentVersion(); return 0;
        }
        auto update=manager.CheckForUpdates();
        if(!update) throw std::runtime_error("No update in fixture");
        if(update->TargetFullRelease.SHA256.size()!=64) throw std::runtime_error("SHA256 missing");
        if(mode=="--reject-corrupt") {
            try { manager.DownloadUpdates(*update); }
            catch(const std::exception &error) {
                std::ofstream output(report); output << error.what(); return 0;
            }
            throw std::runtime_error("CORRUPT PACKAGE WAS ACCEPTED");
        }
        manager.DownloadUpdates(*update);
        manager.WaitExitThenApplyUpdates(*update,true,true,{"--verify",feed,report});
        return 0;
    } catch(const std::exception &error) { std::cerr << error.what() << '\n'; return 1; }
}
