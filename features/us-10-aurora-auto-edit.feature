@wip
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
  start the tool on the night's photos; a tag on a single photo overrides the night's colour for
  that photo.

  @darktable-app
  Rule: An aurora photo gets the aurora look with its own sky brightness and colour

    Scenario: The aurora look
      Given an unedited aurora photo
      When the photos are processed
      Then its edit has the aurora look
      And its sky sits at 0.24 in the edit's rendering
      And its sky background is "neutral blue"
      And every step darktable applied on import is still in its edit

    Scenario: A bright foreground gets a darker sky
      Given an unedited aurora photo whose foreground is at least as bright as its sky
      When the photos are processed
      Then its sky sits at 0.14 in the edit's rendering

    Scenario: The land gets its own colour correction
      Given an unedited aurora photo with land below the horizon
      When the photos are processed
      Then its edit has a colour correction masked to the land
      And the land is "slightly blue snow" in the edit's rendering

    Scenario: A photo of only sky has no land step
      Given an unedited aurora photo without land
      When the photos are processed
      Then its edit has no colour correction for the land

  Rule: A night runs from noon to noon

    Scenario Outline: <situation>
      Given aurora photos taken at <times>
      Then they belong to <nights>

      Examples:
        | situation                      | times                         | nights      |
        | Before and after midnight      | 22:40, 01:15 the next day     | one night   |
        | Morning and evening            | 11:50, 12:10                  | two nights  |
        | Two nights of a trip           | 23:00, 23:30 the next day     | two nights  |

  @darktable
  Rule: Until I choose a night's sky colour, I can compare

    Scenario: An undecided night
      Given a night of 5 unedited aurora photos
      When the photos are processed
      Then their sky backgrounds are "neutral blue"
      And the open step "sky colour: neutral or violet?" lists them

    Scenario Outline: Comparison duplicates
      Given a night of <photos> unedited aurora photos
      When the photos are processed
      Then <duplicates> of them get a duplicate with a "blue-violet" sky, spread over the night

      Examples:
        | photos | duplicates |
        | 1      | 1          |
        | 5      | 3          |
        | 30     | 6          |

  @darktable
  Rule: I choose the colour per night and override it per photo

    Scenario: Choosing the night's colour
      Given an undecided night the tool has edited
      When I tag one of its photos "photoflow|night colour|violet" and start the tool on the night's photos
      Then their sky backgrounds are "blue-violet"
      And the open step "sky colour: neutral or violet?" no longer lists them
      And the comparison duplicates nobody changed are removed

    Scenario: A comparison duplicate I changed stays
      Given an undecided night the tool has edited
      And I changed one of its comparison duplicates by hand
      When I tag one of its photos "photoflow|night colour|neutral" and start the tool on the night's photos
      Then that comparison duplicate is still there

    Scenario: One photo in the other colour
      Given a night the tool has edited in "neutral blue"
      When I tag one of its photos "photoflow|sky colour|violet" and start the tool on it
      Then that photo's sky background is "blue-violet"
      And the other photos of the night stay "neutral blue"

    Scenario: Contradictory night colours are a question
      Given a night whose photos are tagged "photoflow|night colour|violet" and "photoflow|night colour|neutral"
      When the photos are processed
      Then the open step "sky colour: neutral or violet?" lists them

  @darktable-app
  Rule: Where a rule is uncertain, I am asked

    Scenario: The sky target can't be reached
      Given an aurora photo whose sky can't reach its target brightness
      When the photos are processed
      Then the open step "check exposure" lists the photo
