export type RouteName = 'onboarding' | 'chat' | 'models' | 'settings';

class Router {
  route = $state<RouteName>('chat');
  previous = $state<RouteName>('chat');
  sidebarOpen = $state(false);

  navigate(route: RouteName): void {
    this.sidebarOpen = false;
    if (route === this.route) return;
    this.previous = this.route;
    this.route = route;
  }
}

export const router = new Router();
