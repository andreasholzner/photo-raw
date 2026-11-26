@wip
Feature: US-12 — Panorama sets
  As the owner, I mark panorama sets in darktable, and the tool proposes sets I may have missed.

  A panorama set is a tag on its photos, named after the set's first photo, e.g.
  "photoflow|panorama|_DSC6319"; a photo belongs to one set at most. The tool proposes sets from
  photos taken in quick succession with the same focal length (in whole millimetres, rounded); a
  proposal is never stitched until I confirm it (US-13).
  Exposure brackets taken for a panorama simply belong to its set. The actions below are buttons
  in the to-do panel (US-14).

  Rule: I mark panorama sets by hand

    Scenario: Marking the selected photos as a set
      Given three photos in a folder
      When I select them and mark them as a panorama set
      Then all three carry the set tag named after the first of them

    Scenario: A photo marked again moves to the new set
      Given two panorama sets in a folder
      When I select a photo of the first set with the photos of the second and mark them as a set
      Then the photo belongs only to the second set

  Rule: The tool proposes photos taken in quick succession as a set

    Scenario Outline: <situation>
      Given photos taken <seconds> seconds apart with focal lengths <focal lengths>
      When the photos are processed
      Then the tool proposes <proposals>

      Examples:
        | situation                               | seconds  | focal lengths  | proposals     |
        | Four shots in a row                     | 3, 4, 5  | 16, 16, 16, 16 | one set of 4  |
        | Ten seconds apart is still the same set | 10       | 16, 16         | one set of 2  |
        | More than ten seconds apart is not      | 11       | 16, 16         | no set        |
        | A pause splits two sets                 | 3, 30, 3 | 16, 16, 16, 16 | two sets of 2 |
        | A zoom change splits the set            | 3, 3     | 16, 16, 24     | one set of 2  |

    Scenario: Proposals are an open step
      Given a folder with photos taken in quick succession
      When the photos are processed
      Then the open step "panorama sets to confirm" lists them

    Scenario: Photos already in a set aren't proposed
      Given a folder with a panorama set I marked
      When the photos are processed
      Then the tool proposes no set for its photos

  Rule: I confirm, correct or reject a proposal

    Scenario: Choosing a proposal selects its photos
      Given a proposed set
      When I choose it in the to-do panel
      Then darktable selects exactly its photos

    Scenario: Confirming makes the selection the set
      Given a proposed set of four photos
      When I choose it, remove one photo from the selection and confirm
      Then the three selected photos carry the same set tag
      And the fourth photo carries no set or proposal tag
      And the open step "panorama sets to confirm" no longer lists them

    Scenario: A rejected proposal isn't proposed again
      Given a proposed set
      When I choose it and reject it
      And the photos are processed again
      Then the tool proposes no set for these photos
