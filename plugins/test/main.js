import { View } from "gpui-kit";
import { v_flex } from "gpui-base";

/** @import { AsyncContext, Context, Element } from "gpui-kit" */
/** @import { Props } from "gpui-shell" */

export default class Dashboard extends View {
  /** @param {Props | undefined} _props @param {AsyncContext} _cx */
  init(_props, _cx) {}

  /** @param {Context} _cx @returns {Element} */
  render(_cx) {
    console.log("rendering dashboard");
    return v_flex().child("Test");
  }
}
