import 'package:app_tester_companion/features/proxy_safety/data/proxy_safety_repository.dart';
import 'package:app_tester_companion/features/proxy_safety/presentation/proxy_safety_view_model.dart';
import 'package:flutter_test/flutter_test.dart';

class FakeRepository implements ProxySafetyRepository {
  ProxySafetyStatus current =
      const ProxySafetyStatus(isMonitoring: false, isVpnActive: false);

  @override
  Future<ProxySafetyStatus> stopVpn() async => current =
      const ProxySafetyStatus(isMonitoring: false, isVpnActive: false);

  @override
  Future<ProxySafetyStatus> status() async => current;
}

void main() {
  test('loads USB capture status supplied by the desktop intent', () async {
    final repository = FakeRepository();
    final model = ProxySafetyViewModel(repository);

    await model.load();

    expect(model.status?.isMonitoring, isFalse);
    expect(model.status?.isVpnActive, isFalse);
  });

  test('lets the user stop a desktop-started USB VPN', () async {
    final repository = FakeRepository()
      ..current = const ProxySafetyStatus(
        isMonitoring: true,
        isVpnActive: false,
        host: '127.0.0.1',
        port: 8080,
        targetPackage: 'dev.example.app',
      );
    final model = ProxySafetyViewModel(repository);
    await model.load();

    await model.stopVpn();

    expect(model.status?.isVpnActive, isFalse);
  });
}
