// SPDX-License-Identifier: GPL-3.0-or-later
#pragma once
// Shared by the live service and the restart-safety tests.
struct UpdatePolicy {
    bool streaming = false;
    bool downloaded = false;
    bool requested = false;
    bool preparing = false;
    bool canPrepare() const { return downloaded && requested && !streaming && !preparing; }
    void postpone() { requested = false; }
};
