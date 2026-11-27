Feature: US-21 — Night auto-edit
  As the owner, I want night photos without aurora — stars, the Milky Way, moonlit mountains,
  city lights below — edited automatically close to my favourite night edits, so I only correct
  outliers.

  The tool keeps what darktable applied on import and adds the night look (journal 2026-11-26/27,
  rounds 1–13): raw chromatic aberrations, chroma-only and a luma denoise, local contrast
  "clarity" twice and diffuse "local contrast | fine", AgX punchy with a tone curve after it that
  lifts the stars; white balance for slightly blue stars; color balance rgb tints for the dark sky
  and the Milky Way glow; above the horizon a tone equalizer that darkens the sky between the
  stars; below it a brightness step and a colour correction to neutral. The exposure is found per
  photo so that the sky's median sits at 0.065; I raise single photos with the exposure slider.
  The targets for star colour and sky tints are the medians of my six favourite night edits.

  @darktable-app
  Rule: A night photo gets the night look with its own exposure

    Scenario: The night look
      Given an unedited night photo
      When the photos are processed
      Then its edit has the night look
      And its sky sits at 0.065 in the edit's rendering
      And every step darktable applied on import is still in its edit
      And its edit has a brightness step and a colour correction masked to the land

  @darktable-cli
  Rule: A night photo with a bright sky is left to me

    Scenario Outline: <situation>
      Given the reference photo <photo> from <year>
      When the tool looks at its sky
      Then its sky counts as <sky>

      Examples:
        | situation                     | photo        | year | sky    |
        | Stars over a summit cross     | _DSC0906.ARW | 2018 | dark   |
        | Polar-night twilight          | _DSC7332.ARW | 2024 | bright |

    Scenario: The open step for a bright sky
      Given a night photo with a bright sky
      When the night's photos are processed
      Then the open step "night: check look" lists them
      And the photo gets no automatic edit

  Rule: A photo of only sky has no land steps

    Scenario: No land
      Given a night photo without land
      When its night edit is built
      Then its edit has no steps for the land
