# `esp_lcd_touch_gsl3680` — vendored, provenance and licence

## Where this came from

Copied verbatim from the manufacturer's own demo bundle for this exact board:

    JC8012P4A1C_I_W_Y/1-Demo/IDF_5.5.4/JC8012P4A1C_I_W_Y_Old_Panel/
      common_components/esp_lcd_touch_gsl3680/

shipped by Shenzhen Jingcai with the JC8012P4A1C_I_W_Y as the driver for the
GSL3680 touch controller fitted to it.

## Licence status — read before redistributing

**This component carries no LICENSE file and no SPDX headers.** The
manufacturer distributes it as the supporting driver for hardware they sell,
which is the only basis on which it is here.

That is a weaker position than everything else vendored in this directory. The
BSP and the JD9365 panel driver alongside it are Apache-2.0 with SPDX headers;
this one states nothing. It is included because the board cannot do touch
without it and the owner decided, knowingly, that a photo frame you can tap is
worth it.

If you are reusing this repository: this component is the one piece here whose
redistribution has no explicit permission behind it. Removing it costs you the
tap-to-advance gesture and nothing else — `frame_touch.c` fails soft, and the
firmware runs without touch exactly as it did before this was added.

## Local changes

None. Kept byte-identical to the vendor's copy so it can be diffed against
theirs, and so any future bundle can replace it wholesale.
