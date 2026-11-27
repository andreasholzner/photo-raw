Feature: US-23 — Horizon line per photo
  As the owner, I want the tool to find where the land meets the sky in each photo, so that sky
  and foreground can be treated separately without me drawing masks.

  The horizon is a skyline, not a straight line: it follows peaks, ridges, trees and tents. Heights
  are fractions of the frame height from the top; "about" means within 0.05. A photo without any
  land (only sky) has no horizon.

  @darktable-cli
  Rule: The skyline follows the edge of the land

    Scenario Outline: <situation>
      Given the reference photo <photo> from <year>
      When the tool looks for the horizon
      Then the land starts at about <middle> of the frame height
      And the skyline reaches up to about <top>

      Examples:
        | situation                        | photo        | year | middle | top  |
        | Peaks rise high above the line   | _DSC7873.ARW | 2024 | 0.44   | 0.23 |
        | Mountains under an aurora        | _DSC3492.ARW | 2019 | 0.55   | 0.35 |
        | Flat land under a bright aurora  | _DSC3735.ARW | 2019 | 0.72   | 0.61 |
        | A tent in the snow at night      | _DSC7305.ARW | 2024 | 0.69   | 0.59 |
        | A summit cross under the stars   | _DSC0906.ARW | 2018 | 0.96   | 0.82 |
        | Hills in snow at dusk            | _DSC7382.ARW | 2024 | 0.51   | 0.43 |

    @wip
    Scenario: Known gap — an aurora reaching down to the land is taken for land
      Given the reference photo _DSC7579.ARW from 2024
      When the tool looks for the horizon
      Then the land starts at about 0.75 of the frame height
      And the skyline reaches up to about 0.70
