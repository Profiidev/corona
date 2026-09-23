import { View } from "gpui-kit";
import { v_flex, h_flex, Button } from "gpui-base";
import { Button as CompButton, Select } from "gpui-component";
import { greet } from "corona";

/** @import { AsyncContext, Context, Element } from "gpui-kit" */
/** @import { Props } from "gpui-shell" */

export default class Dashboard extends View {
  /** @param {Props | undefined} _props @param {AsyncContext} _cx */
  init(_props, _cx) {
    /** @type {number} */
    this.clicks = 0;
  }

  /** @param {Context} _cx @returns {Element} */
  render(_cx) {
    console.log("rendering dashboard");
    return v_flex()
      .size_full()
      .gap(8)
      .p(8)
      .child(greet("dashboard"))
      .child(`Clicked ${this.clicks} times`)
      .child(
        h_flex()
          .gap(8)
          .child(
            Button.new("bump")
              .on_click((_event, cx) => {
                this.clicks += 1;
                cx.notify();
              })
              .child("Click me"),
          )
          .child(new CompButton("comp").label("test")),
      )
      .child(
        new Select(
          "select",
          () => [
            {
              id: "1",
              label: "test",
            },
            {
              id: "2",
              label: "test2",
            },
            {
              id: "3",
              label: "test3",
            },
            {
              id: "4",
              label: "test4",
            },
            {
              id: "5",
              label: "test5",
            },
          ],
          () => null,
          (_value, _cx) => {},
        ),
      );
  }
}
