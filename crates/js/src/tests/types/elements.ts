// Checked by `typescript_accepts_and_rejects` against kit.d.ts: what type-checks, and the lines
// marked `@ts-expect-error`, which must not.
import { div, type Element, type ElementOf } from 'corona/kit';
import { Switch as BaseSwitch, v_flex } from 'corona/base';
import {
  Button,
  Checkbox,
  Clipboard,
  Field,
  Input,
  InputState,
  Label,
  List,
  RadioGroup,
  Rating,
  Select,
  Tag,
  Toggle
} from 'corona/component';

// component methods on the components that have them, chained with style and events
Button.new('a')
  .label('x')
  .w(1)
  .ghost()
  .on_click((e, cx) => cx.notify());
new Button('a')
  .tooltip('t')
  .px(2)
  .hover((s) => s.bg('primary'));
Checkbox.new('a')
  .checked(true)
  .on_change((on) => {
    const flag: boolean = on;
  });
RadioGroup.new('a').on_change((i) => {
  const index: number = i;
});
Rating.new('a')
  .value(3)
  .on_change((stars) => {
    const n: number = stars;
  });
Select.new(
  'a',
  () => [],
  () => null,
  () => {}
)
  .value('id')
  .placeholder('p');
Tag.new().variant('danger').outline();
div()
  .text_center()
  .child('x')
  .on_click((e, cx) => {});
Field.new().label('l').description('d').align('center');
// @ts-expect-error an Input takes no interaction methods, only its own
Input.new(InputState.new()).role('text_input');
Input.new(InputState.new()).aria_label('search').placeholder('p');

// callbacks get the element they were called on
Button.new('a').when(true, (e) => e.ghost().label('x'));
Button.new('a').map((e) => e.label('x'));
div().when(true, (e) => e.w(1).child('x'));

// elements by what they take
function box(): ElementOf<'style' | 'parent'> {
  return Math.random() > 0.5 ? div() : v_flex();
}
box().w(1).child('x');
function row(): ElementOf<'parent'> {
  return div();
}
row().child('x');

// not on the components that do not have them
// @ts-expect-error a div has no label
div().label('x');
// @ts-expect-error a Label is no Button
Label.new('a').ghost();
// @ts-expect-error a Rating takes a number
Rating.new('a').value('3');
// @ts-expect-error a Select takes the id of a row
Select.new(
  'a',
  () => [],
  () => null,
  () => {}
).value(1);
// @ts-expect-error a Tag has no such look
Tag.new().variant('nope');
// @ts-expect-error no such role
div().role('nope');
// @ts-expect-error a Clipboard takes no style
Clipboard.new('x').w(1);
// @ts-expect-error a base Switch has no label of its own
BaseSwitch.new('a').label('x');
// @ts-expect-error a toggle's change is a flag
Toggle.new('a').on_change((on: string) => {});
// @ts-expect-error not on a div, inside a callback either
div().when(true, (e) => e.label('x'));
// @ts-expect-error an element of unknown kind takes nothing but what every element takes
((e: Element) => e.w(1))(div());
// @ts-expect-error a parent takes no style by being a parent
row().w(1);
// @ts-expect-error a Clipboard is not an element taking style
const styled: ElementOf<'style'> = Clipboard.new('x');

// rows keep their type into the callback
List.new('l', [{ n: 1 }], (row) => div().child(String(row.n)));
// @ts-expect-error the row has no such field
List.new('l', [{ n: 1 }], (row) => div().child(row.m));
Select.new(
  's',
  () => [{ id: 'a', label: 'A' }],
  (row) => div().child(row.label),
  () => {}
);
// @ts-expect-error a select row has no such field
Select.new(
  's',
  () => [{ id: 'a', label: 'A' }],
  (row) => div().child(row.nope),
  () => {}
);
