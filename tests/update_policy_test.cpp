// SPDX-License-Identifier: GPL-3.0-or-later
#include "updatepolicy.h"
#include <iostream>
#include <stdexcept>
void require(bool ok, const char *message) { if(!ok) throw std::runtime_error(message); }
int main() {
    UpdatePolicy p;
    require(!p.canPrepare(), "No unsolicited installation");
    p.downloaded=true; require(!p.canPrepare(), "A cached download is not consent");
    p.requested=true; p.streaming=true; require(!p.canPrepare(), "Never restart an active mirror session");
    p.streaming=false; require(p.canPrepare(), "Install after the session ends, with prior consent");
    p.preparing=true; require(!p.canPrepare(), "Only one apply operation");
    p.preparing=false; p.postpone(); require(!p.canPrepare(), "Later revokes the pending restart request");
    p.requested=true; p.streaming=true; require(!p.canPrepare(), "New frames cancel the idle window");
    p.downloaded=false; p.streaming=false; require(!p.canPrepare(), "Download must finish before installation");
    std::cout << "PASS update consent, streaming interlock, later, and duplicate-apply protection\n";
}
