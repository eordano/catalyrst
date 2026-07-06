# libjpeg9c — vendoring note

Origin: the Independent JPEG Group's libjpeg release 9c (14-Jan-2018). Only the
decoder is exercised, driven at `JDCT_ISLOW` (the integer slow-but-accurate IDCT)
so the JPEG decode path is bit-reproducible across platforms. The IJG license
terms are retained verbatim in `IJG-README` alongside the vendored sources.
