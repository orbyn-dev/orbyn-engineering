/**
 * The one way out of a screen whose code will never arrive: a page reload asks for every chunk again, and a
 * rejected import is kept for the life of the document, so rendering the screen again cannot retry it.
 *
 * Its own module so the reload can be watched in a test without a real navigation, which the test environment
 * cannot make.
 */
export const reloadPage = (): void => window.location.reload();
