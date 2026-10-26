export type RouteName = 'onboarding' | 'chat' | 'models' | 'settings';

class Router {
  route = $state<RouteName>('chat');
  previous = $state<RouteName>('chat');

  navigate(route: RouteName): void {
    if (route === this.route) return;
    this.previous = this.route;
    this.route = route;
  }
}

export const router = new Router();
