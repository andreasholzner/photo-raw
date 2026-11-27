Feature: US-10 — Aurora auto-edit
  As the owner, I want aurora photos edited to show how the aurora felt — bright and colourful,
  not neon — so I can cull after editing.

  The tool keeps what darktable applied on import and adds the aurora look (US-18): chroma-only
  denoise, a luma denoise, local contrast "clarity" twice, sigmoid "ACES 100-nit like" with
  color balance rgb "vibrant colors" and extra chroma. Exposure and white balance are found per
  photo by rendering it, so that the sky has its target brightness and colour; the land below the
  horizon (US-23) gets its own colour correction towards slightly blue snow. The sky is darker
  where the foreground is at least as bright as the sky (a lit tent, moonlit snow).

  The sky colour — neutral blue or blue-violet — is chosen per night: the photos taken from noon
  one day to noon the next. Until I choose, a night's photos get neutral blue, and a few of them
  get a duplicate in blue-violet to compare. I choose with a tag on any photo of the night and
  start the tool on the night's photos, duplicates included; a tag on a single photo overrides the
  night's colour for that photo.

  @darktable-app
  Rule: An aurora photo gets the aurora look with its own sky brightness and colour

    Scenario: The aurora look
      Given an unedited aurora photo
      When the photos are processed
      Then its edit has the aurora look
      And its sky sits at 0.24 in the edit's rendering
      And its sky background is "neutral blue"
      And every step darktable applied on import is still in its edit
      And its edit has a colour correction masked to the land
      And the land is "slightly blue snow" in the edit's rendering

  @darktable-cli
  Rule: A bright foreground gets a darker sky

    Scenario Outline: <situation>
      Given the reference photo <photo> from <year>
      When the tool compares its foreground with its sky
      Then its sky target is <target>

      Examples:
        | situation                  | photo        | year | target |
        | A dark landscape           | _DSC3735.ARW | 2019 | 0.24   |
        | A lit tent in moonlit snow | _DSC7305.ARW | 2024 | 0.14   |

  Rule: A photo of only sky has no land step

    Scenario: No land
      Given an aurora photo without land
      When its aurora edit is built
      Then its edit has no colour correction for the land

  Rule: A night runs from noon to noon

    Scenario Outline: <situation>
      Given aurora photos taken at <times>
      Then they belong to <nights>

      Examples:
        | situation                 | times                     | nights     |
        | Before and after midnight | 22:40, 01:15 the next day | one night  |
        | Morning and evening       | 11:50, 12:10              | two nights |
        | Two nights of a trip      | 23:00, 23:30 the next day | two nights |

  Rule: Until I choose a night's sky colour, I can compare

    @darktable-app
    Scenario: An undecided night
      Given an unedited aurora photo
      When the photos are processed
      Then the open step "sky colour: neutral or violet?" lists the photo
      And it has a comparison duplicate whose sky background is "blue-violet"

    Scenario Outline: A few comparisons, spread over the night
      Given a night of <photos> aurora photos
      Then photos <picked> of them get a comparison duplicate

      Examples:
        | photos | picked                  |
        | 1      | 1                       |
        | 5      | 1, 3, 5                 |
        | 30     | 3, 8, 13, 18, 23, 28    |

  Rule: I choose the colour per night and override it per photo

    @darktable-app
    Scenario: Choosing the night's colour
      Given an aurora photo the tool has edited in an undecided night
      When I tag it "photoflow|night colour|violet" and start the tool on the night's photos
      Then its sky background is "blue-violet"
      And the open step "sky colour: neutral or violet?" doesn't list the photo
      And its comparison duplicate is removed

    @darktable-app
    Scenario: A comparison duplicate I changed stays
      Given an aurora photo the tool has edited in an undecided night
      And I changed its comparison duplicate by hand
      When I tag it "photoflow|night colour|neutral" and start the tool on the night's photos
      Then its comparison duplicate is still there

    @darktable-app
    Scenario: One photo in the other colour
      Given an aurora photo the tool has edited in a "neutral" night
      When I tag it "photoflow|sky colour|violet" and start the tool on it
      Then its sky background is "blue-violet"

    Scenario: Contradictory night colours are a question
      Given a night whose photos are tagged "photoflow|night colour|violet" and "photoflow|night colour|neutral"
      When the night's photos are processed
      Then the open step "sky colour: neutral or violet?" lists them
      And they get no comparison duplicates

  Rule: Where a rule is uncertain, I am asked

    Scenario: The sky target can't be reached
      Given an aurora photo whose sky can't reach its target brightness
      When the night's photos are processed
      Then the open step "check exposure" lists them
