Feature: US-8 — Only touch what was handed over
  As the owner, I want the tool to leave alone everything it wasn't asked to process and every
  edit I made by hand.

  When the tool edits a photo, it tags the photo with a fingerprint of the edit it wrote. As
  long as the photo's edit still matches that fingerprint, the edit is the tool's own and the
  tool may redo it. Changes I make in the darkroom reach darktable's library and the sidecar
  file only when I leave the darkroom; changes in the lighttable arrive at once.

  @darktable
  Rule: The tool edits only photos without a manual edit

    Scenario: A photo nobody has edited yet may be edited
      Given a photo that darktable reports as unedited
      Then the tool may edit it

    Scenario: A photo I edited by hand is left alone
      Given a photo I edited by hand
      Then the tool leaves it alone and reports "edited by hand"

    Scenario: The tool's own edit may be redone
      Given a photo the tool has edited
      Then the tool may edit it

    Scenario Outline: Any change of mine turns the tool's edit into a manual edit
      Given a photo the tool has edited
      When I <change> in the darkroom
      And I leave the darkroom
      Then the tool leaves it alone and reports "edited by hand"

      Examples:
        | change                |
        | change the exposure   |
        | switch a module on    |
        | switch a module off   |

      # A script can't draw a mask: in the self-test this is a check for the owner.
      @manual
      Examples: Masks
        | change                  |
        | draw a mask on a module |

    Scenario: A style I apply in the lighttable is a manual edit too
      Given a photo the tool has edited
      When I apply a style to it in the lighttable
      Then the tool leaves it alone and reports "edited by hand"

    Scenario: Undoing my change in darktable gives the tool its edit back
      Given a photo the tool has edited
      When I change the exposure in the darkroom
      And I undo that change
      And I leave the darkroom
      Then the tool may edit it

    Scenario: Compressing the history is not a change
      Given a photo the tool has edited
      When I compress its history stack
      Then the tool may edit it

  @darktable
  Rule: While darktable shows the darkroom, the tool touches no photo

    Scenario: A change in the darkroom isn't visible to the tool yet
      Given a photo the tool has edited
      When I change the exposure in the darkroom
      Then the tool leaves it alone and reports "darkroom is open"

    Scenario: Also photos I'm not working on wait
      Given a photo that darktable reports as unedited
      And I have another photo open in the darkroom
      Then the tool leaves it alone and reports "darkroom is open"

  @darktable
  Rule: Only photos handed over to the tool are processed
    darktable hands photos over to the tool: newly imported photos at once, others when I start
    the tool on them.

    Scenario: Newly imported photos are processed
      Given I import a folder of new photos
      Then the tool processes exactly these photos

    Scenario: Other photos in the folder are left alone
      Given a folder with photos imported last year
      When I select two of them and start the tool
      Then the tool processes only the two selected photos

  @wip
  Rule: Running the tool again changes nothing that was already right

    Scenario: A second run on the same photos
      Given the tool has processed a folder
      When I start the tool on the same folder again
      Then no photo's edit changes
