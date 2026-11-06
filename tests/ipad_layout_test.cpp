#include "pcmirror_layout.h"
#include <cstdlib>
#include <iostream>
static void require(bool ok) { if (!ok) std::abort(); }
int main() {
    using namespace pcmirror;
    require(is_ipad("iPad14,3") && is_ipad("iPad8,1"));
    require(!is_ipad(nullptr) && !is_ipad("") && !is_ipad("iPhone16,1") && !is_ipad("MacBookPro"));
    for (bool ipad : {false, true}) for (bool fill : {false, true}) {
        auto r = viewport(1920,1080,fill,ipad,4,3);
        require(r.width == 1920);
        require(r.height == ((ipad && fill) ? 1440 : 1080));
        require(r.y == ((ipad && fill) ? -180 : 0));
    }
    auto portrait = viewport(1920,1080,true,true,3,4);
    require(portrait.width == 1920 && portrait.height == 2560 && portrait.y == -740);
    auto tall = viewport(800,1200,true,true,4,3);
    require(tall.width == 1600 && tall.height == 1200 && tall.x == -400);
    auto equal = viewport(1600,1200,true,true,4,3);
    require(equal.width == 1600 && equal.height == 1200 && equal.x == 0 && equal.y == 0);
    auto odd = viewport(1919,1079,true,true,4,3);
    require(odd.width >= 1919 && odd.height >= 1079 && odd.x <= 0 && odd.y <= 0);
    require(viewport(800,600,true,true,0,0).height == 600);
    require(viewport(800,600,true,true,1000000000,1).width == 800);
    std::cout << "PASS iPad-only gating, landscape/portrait, resize, rounding and invalid dimensions\n";
}
