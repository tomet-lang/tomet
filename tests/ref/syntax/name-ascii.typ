A name is ASCII, so a word with a letter outside ASCII in it is text.

Not elements: \@aé(1) \@a-é(1) \@a\_é(1) \@名前(1) \@é(1).

Not calls: \$aé(1) \$a-é(1) \$a.é(1) \$名前(1).

Not boxes: \<?aé\> \<?a-é\> \<?名前\>.

Elements:  and .

Calls: `$a-e(1)` and `$a.e(1)`.

Boxes: \<?a-e\> and \<?a\>.

