Feature: US-7 — Classify day, night and aurora
  As the owner, I want photos sorted into day, night and aurora automatically, and to be asked
  only where it is unclear.

  The scene brightness is the exposure value at ISO 100 (EV100), computed from the camera's
  shutter time, aperture and ISO. My manual 12 mm lens doesn't report its aperture; it is
  counted as f/2.0, the aperture I use it at.

  Rule: The scene brightness decides between day and night

    Scenario Outline: <situation>
      Given a photo taken at <shutter> s, aperture <aperture> and ISO <iso>
      Then its scene brightness is <EV100>
      And it is a <class> photo

      Examples:
        | situation                           | shutter | aperture | iso | EV100 | class   |
        | Sunny mountain view                 | 1/250   | f/8      | 100 | 14.0  | day     |
        | Dusk, handheld                      | 1/30    | f/4      | 800 | 5.9   | unclear |
        | Aurora with the manual lens         | 4       | manual   | 400 | -2.0  | night   |
        | Night limit: EV100 1 is still night | 2       | f/2      | 100 | 1.0   | night   |
        | Day limit: EV100 8 is already day   | 1       | f/16     | 100 | 8.0   | day     |

    @wip
    Scenario: An unclear photo becomes a question
      Given a photo with scene brightness 5
      When the photos are processed
      Then the open step "day or night?" lists the photo

  @wip @darktable-cli
  Rule: Night photos are checked for aurora

    Scenario: Colour in the night sky means aurora
      Given a night photo with a green band in the sky
      When the photos are processed
      Then it is an aurora photo

    Scenario: A night photo without aurora becomes a question
      Given a night photo with a dark, colourless sky
      When the photos are processed
      Then the open step "night photo without aurora" lists the photo

