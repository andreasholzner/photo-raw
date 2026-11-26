# Links

References used in the analysis and planning (see [journal.md](journal.md) for context).

## darktable

- [darktable 5.6 user manual — processing modules](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/)
  (includes the list of deprecated modules)
- [sigmoid (5.6)](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/sigmoid/)
  — use exposure for mid-tones first; never combine with another display transform
- [AgX (5.6)](https://docs.darktable.org/usermanual/5.6/en/module-reference/processing-modules/agx/)
- [filmic rgb](https://docs.darktable.org/usermanual/development/en/module-reference/processing-modules/filmic-rgb/)
- [scene-referred workflow (3.6 manual, partly outdated)](https://docs.darktable.org/usermanual/3.6/en/overview/workflow/edit-scene-referred/)
- [darktable's color pipeline](https://www.darktable.org/usermanual/en/special-topics/color-pipeline/)
- [haze removal](https://docs.darktable.org/usermanual/3.8/en/module-reference/processing-modules/haze-removal/)
  — meant for real haze; increases contrast; fails on images without haze
- [diffuse or sharpen](https://docs.darktable.org/usermanual/development/en/module-reference/processing-modules/diffuse/)
- [darktable 5.6 user manual (PDF)](https://docs.darktable.org/usermanual/5.6/en/darktable_user_manual.pdf)
  — astrophoto denoise (for colour and luma noise separately), profiled denoise, color calibration
- [Linux Magazine: darktable 5.4's scene-referred workflow](https://www.linux-magazine.com/Issues/2026/306/filmic-rgb)
- [darktable Lua API documentation](https://docs.darktable.org/lua/stable/)
- [darktable releases (5.6.1 AppImage used in the dev container)](https://github.com/darktable-org/darktable/releases/tag/release-5.6.1)

## Aurora editing

- [Digital Photography School: editing aurora photos in Lightroom](https://digital-photography-school.com/8-lightroom-controls-aurora-editing)
  — warns about over-saturation
- [National Parks at Night: editing auroras](https://nationalparksatnight.com/blog/2023/4/1/editing-auroras-how-to-post-process-your-northern-lights-photos)
- [Sky at Night: how to edit aurora astrophotos](https://www.skyatnightmagazine.com/astrophotography/astrophoto-tips/how-edit-aurora-astrophotos)
- [Sky at Night: balanced auroral landscapes](https://www.skyatnightmagazine.com/astrophotography/astrophoto-tips/how-to-create-balanced-auroral-landscapes)
- [Space.com: how to edit aurora photos](https://space.com/how-to-edit-aurora-photos)
- [Nikon: 5 tips for better northern lights photographs](https://www.nikon.co.uk/en_GB/learn-and-explore/magazine/tips-and-tricks/5-tips-for-better-northern-lights-photographs)
- [Fstoppers: six-step program to processing the aurora](https://fstoppers.com/originals/advanced-six-step-program-processing-aurora-lightroom-and-photoshop-164984)
- [PetaPixel: forecasting, shooting and post-processing the northern lights](https://petapixel.com/2018/10/02/the-ultimate-guide-to-forecasting-shooting-and-post-processing-the-northern-lights)

## Night (Milky Way, stars) editing

- [pixls.us: resources for editing astrophotography](https://discuss.pixls.us/t/resources-for-editing-astrophotography/43340)
  — darktable: color calibration on the light-pollution halo, global contrast with sigmoid
- [PetaPixel: from a flat, noisy RAW to a finished Milky Way photograph (2026)](https://petapixel.com/2026/06/24/how-to-turn-a-flat-noisy-raw-into-a-finished-milky-way-photograph/)
  — every contrast step risks bringing noise back
- [Photography Life: how to edit Milky Way photos](https://photographylife.com/how-to-edit-milky-way-lightroom)
- [Expert Photography: editing Milky Way photos](https://expertphotography.com/post-processing-milky-way-photography-settings-presets)
  — white balance first: near neutral, slightly towards blue/magenta; dehaze with restraint; no
  sharpening
- [Nature TTL: 5 mistakes to avoid when processing star photos](https://www.naturettl.com/5-mistakes-to-avoid-when-processing-star-photos/)
- [Fstoppers: 8 steps to better Milky Way editing](https://fstoppers.com/astrophotography/8-steps-better-milky-way-editing-lightroom-515949)
  — generally no sharpening on Milky Way shots
- [PetaPixel: 6 editing mistakes in landscape photos (2024)](https://petapixel.com/2024/02/04/6-photo-editing-mistakes-i-wish-i-was-warned-about-earlier)
  — over-saturation as the most common sky-editing error
- [Sky at Night: 12 common astrophotography mistakes](https://www.skyatnightmagazine.com/astrophotography/astrophoto-tips/common-mistakes)
- [Capture the Atlas academy: star reduction](https://academy.capturetheatlas.com/?p=25573)
  — fewer, smaller stars to let the Milky Way stand out and the sky look less noisy
- [Reddit r/ExposurePorn: before/after star removal (2025)](https://www.reddit.com/r/ExposurePorn/comments/1l8hqm1/)
- [AstroBin forum: how far is it acceptable to reduce stars?](https://app.astrobin.com/forum/topic/66598/processing/how-far-is-it-acceptable-to-reduce-stars)
  — deep-sky view; star reduction is a matter of taste and easily overdone
