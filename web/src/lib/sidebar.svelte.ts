/** The device's sidebar choice, shared by the sidebar and the Appearance settings. */
import { readSidebar, saveSidebar, type SidebarChoice } from '#lib/sidebar.ts';

export const sidebar = $state<{ choice: SidebarChoice }>({ choice: readSidebar() });

export function chooseSidebar(choice: SidebarChoice): void {
	sidebar.choice = choice;
	saveSidebar(choice);
}
