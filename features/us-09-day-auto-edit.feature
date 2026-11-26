@wip
Feature: US-9 — Day auto-edit
  As the owner, I want day photos to get my base edit with a per-photo exposure, so I only
  correct outliers.

  The tool keeps what darktable applied on import (its defaults and my own automatic presets,
  such as lens correction and denoise) and adds the day look on top: AgX "punchy" by default,
  sigmoid "ACES 100-nit like" with color balance rgb "vibrant colors" when I switch to it, local
  contrast "clarity", and an exposure found per photo by rendering it. The edit contains no
  sharpening: that comes with the print export (US-5). I switch a photo with a tag and start the
  tool on it again; the tags stay, so my choices can later teach a look rule.

  @darktable-cli
  Rule: A day photo gets the day look with its own exposure

    Scenario: The default look
      Given an unedited day photo
      When the photos are processed
      Then its edit has the look "AgX punchy" and local contrast "clarity"
      And its brightest 10 % sit at 0.675 in the edit's rendering
      And its edit contains no sharpening

    Scenario: What darktable applied on import is kept
      Given an unedited day photo
      When the photos are processed
      Then every step darktable applied on import is still in its edit

  @darktable-cli
  Rule: I switch the look and dehaze with tags

    Scenario: Switching to sigmoid
      Given a day photo the tool has edited
      When I tag it "photoflow|look|sigmoid" and start the tool on it
      Then its edit has the look "sigmoid ACES with vibrant colors" and local contrast "clarity"
      And its brightest 1 % sit at 0.885 in the edit's rendering

    Scenario: Switching dehaze on
      Given a day photo the tool has edited
      When I tag it "photoflow|dehaze|on" and start the tool on it
      Then its edit also has diffuse or sharpen "dehaze"

  @darktable-cli
  Rule: Where a rule is uncertain, I am asked

    Scenario: The exposure target can't be reached
      Given a day photo whose brightest parts are clipped
      When the photos are processed
      Then the open step "check exposure" lists the photo

    Scenario: A snow scene in AgX
      Given an unedited day photo of a bright snow scene
      When the photos are processed
      Then the open step "snow: check look" lists the photo

    Scenario: A snow scene already switched to sigmoid
      Given a day photo of a bright snow scene the tool has edited
      When I tag it "photoflow|look|sigmoid" and start the tool on it
      Then the open step "snow: check look" doesn't list the photo

  Rule: After a darktable upgrade the tool edits nothing until the self-test is accepted

    Scenario: darktable was upgraded
      Given the self-test was last accepted with darktable 5.6.1
      And darktable 5.8.0 is installed
      When the photos are processed
      Then the tool edits no photo and reports "run the self-test after the darktable upgrade"
